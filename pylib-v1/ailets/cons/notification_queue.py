import asyncio
from dataclasses import dataclass
from typing import Dict, Set
import threading


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

    async def wait_for_handle(
        self, handle: int, release_before_wait: threading.Lock
    ) -> None:
        client = WaitingClient.new()

        with self._lock:
            if handle not in self._waiting_clients:
                self._waiting_clients[handle] = set()
            self._waiting_clients[handle].add(client)

        try:
            release_before_wait.release()
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
