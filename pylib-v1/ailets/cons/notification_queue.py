import asyncio
from dataclasses import dataclass
from typing import Dict, Set
import threading

"""

In the first approximation, the workflow is as follows:

10. Client: check condition
20. Client: call `wait_for_handle`
30. Queue-for-client: add client to the waiting list
40. Queue-for-client: wait for handle notification

50. Worker: call `notify`
60. Queue-for-worker: extract the client(s) from the waiting list
70. Queue-for-worker: notify the event loop to awake the client(s)

80. Queue-for-client: awake and exit from `wait_for_handle`

However, due to the worker being in a different thread,
the step 60 "extract the client(s) from the waiting list" can happen
before the step 30 "add client to the waiting list". This way, the client
will not be notified about the handle event and will wait indefinitely.

To avoid this, the client should aquire the lock to make the steps 10-30 atomic.

To hold the lock as little as possible, here is the suggested client workflow:

```
if should_wait():
  do_something_preliminary()

lock = queue.get_lock()
with lock:
    if should_wait():
        queue.wait_for_handle(handle)
        lock.acquire()  # re-aquire the lock to match the release in `wait_for_handle`
```

"""


@dataclass(frozen=True)
class WaitingClient:
    """Represents a client waiting for a handle notification"""

    loop: asyncio.AbstractEventLoop
    event: asyncio.Event

    @classmethod
    def new(cls) -> "WaitingClient":
        return cls(loop=asyncio.get_running_loop(), event=asyncio.Event())


class NotificationQueue:
    """Thread-safe queue for handle (as integers) notifications"""

    def __init__(self) -> None:
        self._waiting_clients: Dict[int, Set[WaitingClient]] = {}
        self._lock = threading.Lock()

    def get_lock(self) -> threading.Lock:
        return self._lock

    async def wait_for_handle(self, handle: int) -> None:
        """Wait for the handle notification
        The caller should aquire the lock before calling this method.
        See the module documentation for more details.
        """
        client = WaitingClient.new()

        if handle not in self._waiting_clients:
            self._waiting_clients[handle] = set()
        self._waiting_clients[handle].add(client)

        try:
            self._lock.release()
            await client.event.wait()
        finally:
            # Clean up the client registration
            with self._lock:
                if handle in self._waiting_clients:
                    self._waiting_clients[handle].discard(client)
                    if not self._waiting_clients[handle]:
                        del self._waiting_clients[handle]

    def notify(self, handle: int) -> None:
        with self._lock:
            clients = self._waiting_clients.get(handle, set()).copy()
        for client in clients:
            client.loop.call_soon_threadsafe(client.event.set)


class DummyNotificationQueue:
    def get_lock(self) -> threading.Lock:
        return threading.Lock()

    def notify(self, handle: int) -> None:
        pass

    async def wait_for_handle(self, handle: int) -> None:
        pass
