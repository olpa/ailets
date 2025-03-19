import asyncio
import io
import logging
import threading
from typing import Optional

from .notification_queue import NotificationQueue
from .atyping import INotificationQueue

logger = logging.getLogger("ailets.io")


class BufWriterWithState:
    def __init__(
        self, handle: int, buffer: io.BytesIO, queue: INotificationQueue
    ) -> None:
        self.handle = handle
        self.buffer = buffer  # shared between threads with readers
        self.queue = queue
        self.lock = threading.Lock()  # for `.buffer`
        self.error: Optional[Exception] = None
        self._is_closed = False
        self.pos = 0

    def get_handle(self) -> int:
        return self.handle

    def get_lock(self) -> threading.Lock:
        return self.lock

    def get_pos(self) -> int:
        return self.pos

    def write(self, data: bytes) -> int:
        # `.buffer` is shared between threads with readers, so we need a lock
        with self.lock:
            n = self.buffer.write(data)
            self.pos += n
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
            if self.writer is not None and self.pos >= self.writer.get_pos():
                await self._wait_for_writer()
                continue

            buffer = self.buffer.getvalue()

            if size < 0:
                end_pos = len(buffer)
            else:
                end_pos = self.pos + size
            data = buffer[slice(self.pos, end_pos)]
            self.pos = end_pos
            return data

        if self.error is not None:
            raise self.error
        return b""

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
        lock = self.writer.get_lock()
        with lock:
            if self.pos >= self.writer.get_pos():
                await self.queue.wait_for_handle(self.writer.get_handle(), lock)
                # Re-acquire lock to match release in `wait_for_handle`
                lock.acquire()


def main() -> None:
    async def writer(lib_writer: BufWriterWithState) -> None:
        try:
            while True:
                s = await asyncio.to_thread(input)
                s = s.strip()
                if not s:
                    break
                lib_writer.write(s.encode("utf-8"))
        except EOFError:
            pass
        finally:
            lib_writer.close()

    async def reader(name: str, lib_reader: BufReaderFromPipe) -> None:
        while True:
            data = await lib_reader.read(size=4)
            if data is None:
                raise lib_reader.get_error() or EOFError()
            if len(data) == 0:
                break
            print(f"({name}): {data.decode()}")

    async def main() -> None:
        queue = NotificationQueue()
        buffer = io.BytesIO()
        lib_writer = BufWriterWithState(0, buffer, queue)
        lib_reader1 = BufReaderFromPipe(1, buffer, lib_writer, queue)
        lib_reader2 = BufReaderFromPipe(2, buffer, lib_writer, queue)
        lib_reader3 = BufReaderFromPipe(3, buffer, lib_writer, queue)

        writer_task = asyncio.create_task(writer(lib_writer))
        rt1 = asyncio.create_task(reader("r1", lib_reader1))
        rt2 = asyncio.create_task(reader("r2", lib_reader2))
        rt3 = asyncio.create_task(reader("r3", lib_reader3))

        await asyncio.gather(writer_task, rt1, rt2, rt3)

    logging.basicConfig(level=logging.DEBUG)
    asyncio.run(main())


if __name__ == "__main__":
    main()
