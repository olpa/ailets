import asyncio
from dataclasses import dataclass
import io
import logging
from typing import Callable, Optional, Set

from .notification_queue import NotificationQueue
from .atyping import INotificationQueue

logger = logging.getLogger("ailets.io")


class BufWriterWithState:
    def __init__(self, handle: int, buffer: io.BytesIO, queue: INotificationQueue) -> None:
        self.handle = handle
        self.buffer = buffer
        self.queue = queue
        self.error: Optional[Exception] = None
        self._is_closed = False

    def get_handle(self) -> int:
        return self.handle

    def write(self, data: bytes) -> int:
        n = self.buffer.write(data)
        assert n == len(data), f"written bytes ({n}) != expected ({len(data)})"
        self.queue.notify(self.handle)
        return n

    def get_error(self) -> Optional[Exception]:
        return self.error

    def is_closed(self) -> bool:
        return self._is_closed

    def close(self) -> None:
        self._is_closed = True
        self.queue.notify(self.handle)


class BufReaderFromPipe:
    def __init__(
        self,
        handle: int,
        buffer: io.BytesIO,
        writer: Optional[BufWriterWithState],
        queue: INotificationQueue,
    ) -> None:
        self.handle = handle
        self.buffer = buffer
        self.writer = writer
        self.queue = queue
        self.error: Optional[Exception] = None
        self.pos = 0
        self._is_closed = False

    async def read(self, size: int = -1) -> Optional[bytes]:
        while self.error is None and not self.is_closed():
            # TODO FIXME: potential race condition:
            # a write can happen between the check and the wait, we will miss it
            buffer = self.buffer.getvalue()
            
            if self.pos >= len(buffer) and self.writer is not None:
                await self._wait_for_writer()
                continue

            if size < 0:
                end_pos = len(buffer)
            else:
                end_pos = self.pos + size
            data = buffer[slice(self.pos, end_pos)]
            self.pos = end_pos
            return data

        if self.error is not None:
            raise self.error
        return b''

    def is_closed(self) -> bool:
        return self._is_closed

    def get_error(self) -> Optional[Exception]:
        return self.error

    def close(self) -> None:
        self._is_closed = True

    async def _wait_for_writer(self) -> None:
        if self.writer is None:
            return
        error = self.writer.get_error()
        if error is not None:
            self.error = error
            return
        if self.writer.is_closed():
            self.close()
            return
        await self.queue.wait_for_handle(self.writer.get_handle())


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
        self.buffer = io.BytesIO()
        if initial_content:
            self.buffer.write(initial_content)
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
        old_pos = self.buffer.tell()
        self.buffer.seek(0, io.SEEK_END)
        n = self.buffer.write(data)
        new_pos = self.buffer.tell()
        self.buffer.seek(old_pos)
        logger.debug(
            "Buffer write%s: pos %d->%d",
            f" ({self.debug_hint})" if self.debug_hint else "",
            old_pos,
            new_pos,
        )
        self.notify_readers()
        if old_pos == 0 and new_pos > 0:
            self.on_write_started()
        return n

    async def read(self, pos: int, size: int = -1) -> bytes:
        reader_sync = ReaderSync.new()
        try:
            self.reader_sync.add(reader_sync)
            self.buffer.seek(0, io.SEEK_END)
            while self.buffer.tell() <= pos:
                if self.is_closed():
                    return b""
                await reader_sync.event.wait()
                reader_sync.event.clear()
        finally:
            self.reader_sync.remove(reader_sync)

        self.buffer.seek(pos)
        if size < 0:
            data = self.buffer.read()
        else:
            data = self.buffer.read(size)

        logger.debug(
            "Buffer read%s: pos %d->%d",
            f" ({self.debug_hint})" if self.debug_hint else "",
            pos,
            pos + len(data),
        )

        return data


def main() -> None:
    async def writer(lib_writer: BufWriterWithState) -> None:
        try:
            while True:
                print("!!!! writer: before input")  # FIXME
                s = await asyncio.to_thread(input)
                print("!!!! writer: after input")  # FIXME
                s = s.strip()
                if not s:
                    break
                lib_writer.write(s.encode("utf-8"))
        except EOFError:
            print("!!!! writer: EOFError")  # FIXME
            pass
        finally:
            print("!!!! writer: finally")  # FIXME
            lib_writer.close()

    async def reader(name: str, lib_reader: BufReaderFromPipe) -> None:
        while True:
            print(f"!!!! reader ({name}): before read")  # FIXME
            data = await lib_reader.read(size=4)
            print(f"!!!! reader ({name}): after read: {data!r}")  # FIXME
            if data is None:
                raise lib_reader.get_error() or EOFError()
            if len(data) == 0:
                break
            print(f"({name}): {data.decode()}")

    async def main() -> None:
        queue = NotificationQueue()
        buffer = io.BytesIO()
        lib_writer = BufWriterWithState(0, buffer, queue)
        lib_reader = BufReaderFromPipe(1, buffer, lib_writer, queue)

        writer_task = asyncio.create_task(writer(lib_writer))
        rt1 = asyncio.create_task(reader("r1", lib_reader))
        rt2 = asyncio.create_task(reader("r2", lib_reader))
        rt3 = asyncio.create_task(reader("r3", lib_reader))

        print("!!!!!!!!!!!!!! before gather")  # FIXME
        await asyncio.gather(writer_task, rt1, rt2, rt3)
        print("!!!!!!!!!!!!!! after gather")  # FIXME

    logging.basicConfig(level=logging.DEBUG)
    asyncio.run(main())


if __name__ == "__main__":
    main()
