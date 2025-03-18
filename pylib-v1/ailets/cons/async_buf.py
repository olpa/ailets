import asyncio
from dataclasses import dataclass
import logging
from typing import Callable, Optional, Set

logger = logging.getLogger("ailets.io")


class BufWriterWithState:
    def __init__(self, buffer: bytes) -> None:
        self.buffer = buffer
        self.error: Optional[Exception] = None
        self._is_closed = False

    def write(self, data: bytes) -> int:
        self.buffer += data
        return len(data)

    def get_error(self) -> Optional[Exception]:
        return self.error

    def is_closed(self) -> bool:
        return self._is_closed

    def close(self) -> None:
        self._is_closed = True


class BufReaderFromPipe:
    def __init__(self, buffer: bytes) -> None:
        self.buffer = buffer
        self.error: Optional[Exception] = None
        self.pos = 0
        self._is_closed = False

    def read(self, size: int = -1) -> Optional[bytes]:
        while self.error is None and not self.is_closed():
            if self.pos >= len(self.buffer):
                self._wait_for_data()
                continue

            size = len(self.buffer) - self.pos
            if size == 0:
                return b""
            end_pos = self.pos + size
            data = self.buffer[slice(self.pos, end_pos)]
            self.pos = end_pos
            return data

        if self.error is not None:
            raise self.error
        return None

    def is_closed(self) -> bool:
        return self._is_closed

    def close(self) -> None:
        self._is_closed = True

    def _wait_for_data(self) -> None:
        pass


@dataclass(frozen=True)
class ReaderSync:
    loop: asyncio.AbstractEventLoop
    event: asyncio.Event

    @classmethod
    def new(cls) -> "ReaderSync":
        return cls(loop=asyncio.get_event_loop(), event=asyncio.Event())


class AsyncBuffer:
    def __init__(
        self,
        initial_content: Optional[bytes],
        is_closed: bool,
        on_write_started: Callable[[], None],
        debug_hint: Optional[str] = None,
    ) -> None:
        self.buffer = initial_content or b""
        self._is_closed = is_closed
        self.on_write_started = on_write_started
        self.debug_hint = debug_hint
        self.reader_sync: Set[ReaderSync] = set()

    def notify_readers(self) -> None:
        # copy to avoid race condition (Set changed size during iteration)
        readers = self.reader_sync.copy()
        for reader in readers:
            if reader in self.reader_sync:
                reader.loop.call_soon_threadsafe(reader.event.set)

    async def close(self) -> None:
        self._is_closed = True
        self.notify_readers()
        logger.debug(
            "Buffer closed%s", f" ({self.debug_hint})" if self.debug_hint else ""
        )

    def is_closed(self) -> bool:
        return self._is_closed

    async def write(self, data: bytes) -> int:
        old_pos = len(self.buffer)
        self.buffer += data
        new_pos = len(self.buffer)
        logger.debug(
            "Buffer write%s: pos %d->%d",
            f" ({self.debug_hint})" if self.debug_hint else "",
            old_pos,
            new_pos,
        )
        self.notify_readers()
        if old_pos == 0 and new_pos > 0:
            self.on_write_started()
        return len(data)

    async def read(self, pos: int, size: int = -1) -> bytes:
        reader_sync = ReaderSync.new()
        try:
            self.reader_sync.add(reader_sync)
            while len(self.buffer) <= pos:
                if self.is_closed():
                    return b""
                await reader_sync.event.wait()
                reader_sync.event.clear()
        finally:
            self.reader_sync.remove(reader_sync)

        if size < 0:
            return self.buffer[pos:]
        end = pos + size
        if end > len(self.buffer):
            end = len(self.buffer)

        logger.debug(
            "Buffer read%s: pos %d->%d",
            f" ({self.debug_hint})" if self.debug_hint else "",
            pos,
            end,
        )

        return self.buffer[pos:end]


if __name__ == "__main__":

    async def writer(buffer: AsyncBuffer) -> None:
        try:
            while True:
                s = await asyncio.to_thread(input)
                s = s.strip()
                if not s:
                    break
                await buffer.write(s.encode("utf-8"))
        except EOFError:
            pass
        finally:
            await buffer.close()

    async def reader(name: str, buffer: AsyncBuffer) -> None:
        pos = 0
        while True:
            data = await buffer.read(pos, size=4)
            size = len(data)
            pos += size

            if size == 0:
                break
            print(f"({name}): {data.decode()}")

    async def main() -> None:
        buffer = AsyncBuffer(b"", False, lambda: None)
        writer_task = asyncio.create_task(writer(buffer))
        rt1 = asyncio.create_task(reader("r1", buffer))
        rt2 = asyncio.create_task(reader("r2", buffer))
        rt3 = asyncio.create_task(reader("r3", buffer))

        await asyncio.gather(writer_task, rt1, rt2, rt3)

    logging.basicConfig(level=logging.DEBUG)
    asyncio.run(main())
