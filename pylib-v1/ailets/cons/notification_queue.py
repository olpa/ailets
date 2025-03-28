import asyncio
from dataclasses import dataclass
import logging
from typing import Any, Callable, Dict, Optional, Protocol, Set
import threading

"""

1) Waiting for a handle

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
        queue.wait_unsafe(handle, debug_hint)
        lock.acquire()  # re-aquire the lock to match the release in `wait_for_handle`
```

2) Subscribing to a handle

Nothing special here.

"""

logger = logging.getLogger("ailets.queue")


class INotificationQueue(Protocol):
    def get_lock(self) -> threading.Lock:
        raise NotImplementedError

    def notify(self, handle: int, arg: int) -> None:
        raise NotImplementedError

    def whitelist(self, handle: int, debug_hint: str) -> None:
        raise NotImplementedError

    def unlist(self, handle: int) -> None:
        raise NotImplementedError

    async def wait_unsafe(self, handle: int, debug_hint: str) -> None:
        raise NotImplementedError

    def subscribe(
        self, handle: int, func: Callable[[int], Any], debug_hint: str
    ) -> Optional[int]:
        raise NotImplementedError

    def unsubscribe(self, handle: int, subscription_id: int) -> None:
        raise NotImplementedError


@dataclass(frozen=True)
class WaitingClient:
    """Represents a client waiting for a handle notification"""

    loop: asyncio.AbstractEventLoop
    event: asyncio.Event
    debug_hint: str

    @classmethod
    def new(cls, debug_hint: str) -> "WaitingClient":
        return cls(
            loop=asyncio.get_running_loop(),
            event=asyncio.Event(),
            debug_hint=debug_hint,
        )

    def __str__(self) -> str:
        return f"WaitingClient({self.debug_hint})"


@dataclass(frozen=True)
class SubscribedClient:
    """Represents a client subscribed to handle notifications"""

    func: Callable[[int], Any]
    debug_hint: str

    @classmethod
    def new(
        cls,
        func: Callable[[int], Any],
        debug_hint: str,
    ) -> "SubscribedClient":
        return cls(
            func=func,
            debug_hint=debug_hint,
        )

    def __str__(self) -> str:
        return f"SubscribedClient({self.debug_hint})"


class NotificationQueue(INotificationQueue):
    """Thread-safe queue for handle (as integers) notifications"""

    def __init__(self) -> None:
        self._waiting_clients: Dict[int, Set[WaitingClient]] = {}
        self._subscribed_clients: Dict[int, Set[SubscribedClient]] = {}
        self._lock = threading.Lock()
        self._whitelist: Dict[int, str] = {}

    def get_lock(self) -> threading.Lock:
        return self._lock

    def whitelist(self, handle: int, debug_hint: str) -> None:
        with self._lock:
            if handle in self._whitelist:
                logger.warning(
                    "queue.whitelist: handle %s already in whitelist", handle
                )
            self._whitelist[handle] = debug_hint

    def unlist(self, handle: int) -> None:
        with self._lock:
            if handle not in self._whitelist:
                logger.warning("queue.unlist: handle %s not in whitelist", handle)
            del self._whitelist[handle]
        self._notify_and_delete(handle, arg=-1, delete_subscribed=True)

    def subscribe(
        self, handle: int, func: Callable[[int], Any], debug_hint: str
    ) -> Optional[int]:
        """Subscribe to the handle notification

        Returns:
            The handle id of the subscription, to unsubscribe later.
        """
        with self._lock:
            if handle not in self._whitelist:
                logger.warning(f"queue.subscribe: handle {handle} not in whitelist")
                return None
            client = SubscribedClient.new(func, debug_hint)
            if handle not in self._subscribed_clients:
                self._subscribed_clients[handle] = set()
            self._subscribed_clients[handle].add(client)
            return id(client)

    def unsubscribe(self, handle: int, subscription_id: int) -> None:
        with self._lock:
            if handle not in self._subscribed_clients:
                logger.warning(
                    f"queue.unsubscribe: handle {handle} not in subscribed clients"
                )
                return
            subscriptions = self._subscribed_clients[handle]
            subscription = next(
                (s for s in subscriptions if id(s) == subscription_id), None
            )
            if subscription is None:
                logger.warning(
                    f"queue.unsubscribe: subscription {subscription_id} "
                    f"for handle {handle} not found"
                )
                return
            subscriptions.discard(subscription)
            if not subscriptions:
                del self._subscribed_clients[handle]

    async def wait_unsafe(self, handle: int, debug_hint: str) -> None:
        """Wait for the handle notification

        Precondition: The caller should aquire the lock before calling this method.
        Post-condition: The lock is released after the method returns.

        See the module documentation for more details.
        The word "unsafe" in the method name hints that the caller should
        read the documentation.
        """
        logger.debug("queue.wait_unsafe: %s", handle)
        if handle not in self._whitelist:
            # Don't warn: the whole idea of whitelist is to
            # avoid waiting in case of race conditions
            self._lock.release()
            return

        client = WaitingClient.new(debug_hint=debug_hint)

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

    def notify(self, handle: int, arg: int) -> None:
        self._notify_and_delete(handle, arg=arg, delete_subscribed=False)

    def _notify_and_delete(
        self, handle: int, arg: int, delete_subscribed: bool
    ) -> None:
        with self._lock:
            clients1 = self._waiting_clients.get(handle, set())
            if handle in self._waiting_clients:
                del self._waiting_clients[handle]
            if delete_subscribed:
                if handle in self._subscribed_clients:
                    clients2 = self._subscribed_clients[handle]
                    del self._subscribed_clients[handle]
                else:
                    clients2 = set()
            else:
                clients2 = self._subscribed_clients.get(handle, set()).copy()
        logger.debug(
            "queue.notify: handle %s, len(clients1): %s, len(clients2): %s",
            handle,
            len(clients1),
            len(clients2),
        )
        for client1 in clients1:
            client1.loop.call_soon_threadsafe(client1.event.set)
        for client2 in clients2:
            try:
                client2.func(arg)
            except Exception as e:
                logger.exception("queue.notify: error in client %s: %s", client2, e)

    def get_waits(self) -> list[tuple[int, list[str]]]:
        with self._lock:
            return [
                (handle, [f"{str(client)}@{id(client)}" for client in clients])
                for handle, clients in self._waiting_clients.items()
            ]


class DummyNotificationQueue(INotificationQueue):
    def get_lock(self) -> threading.Lock:
        return threading.Lock()

    def notify(self, handle: int, arg: int) -> None:
        pass

    async def wait_unsafe(self, handle: int, debug_hint: str) -> None:
        pass

    def get_waits(self) -> list[tuple[int, list[str]]]:
        return []

    def whitelist(self, handle: int, debug_hint: str) -> None:
        pass

    def unlist(self, handle: int) -> None:
        pass

    def subscribe(
        self, handle: int, func: Callable[[int], Any], debug_hint: str
    ) -> Optional[int]:
        return None

    def unsubscribe(self, handle: int, subscription_id: int) -> None:
        pass
