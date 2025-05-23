import asyncio
import errno
import logging
import threading
from typing import Optional

from ailets.atyping import IAsyncReader, IAsyncWriter
from ailets.cons.notification_queue import INotificationQueue, NotificationQueue

logger = logging.getLogger("ailets.mempipe")


class Writer(IAsyncWriter):
    def __init__(
        self,
        handle: int,
        queue: INotificationQueue,
        debug_hint: str,
        external_buffer: Optional[bytearray] = None,
    ) -> None:
        super().__init__()
        self.errno = [0]
        self.buffer = external_buffer if external_buffer is not None else bytearray()
        self.handle = handle
        self.queue = queue
        self.debug_hint = debug_hint
        self.closed = False
        self.queue.whitelist(handle, f"MemPipe.Writer {debug_hint}")
        self.close_lock = threading.Lock()

    def __str__(self) -> str:
        return (
            f"MemPipe.Writer(handle={self.handle}, "
            f"closed={self.closed}, "
            f"tell={self.tell()}, "
            f"hint={self.debug_hint})"
        )

    def get_error(self) -> int:
        return self.errno[0]

    def set_error(self, errno: int) -> None:
        if self.closed:
            return
        self.errno[0] = errno
        self.queue.notify(self.handle, errno)

    async def write(self, data: bytes) -> int:
        return self.write_sync(data)

    def write_sync(self, data: bytes) -> int:
        if self.closed:
            raise OSError(errno.EBADF, "Writer is closed")
        if ecode := self.get_error():
            raise OSError(ecode, "Writer is in an error state")

        if len(data) == 0:
            return 0
        self.buffer.extend(data)
        self.queue.notify(self.handle, len(data))
        return len(data)

    def tell(self) -> int:
        return len(self.buffer)

    def close(self) -> None:
        with self.close_lock:
            self.closed = True
        self.queue.notify(self.handle, -1)
        self.queue.unlist(self.handle)


class Reader(IAsyncReader):
    def __init__(self, handle: int, writer: Writer) -> None:
        super().__init__()
        self.handle = handle
        self.writer = writer
        self.pos = 0
        self.closed = False

    def close(self) -> None:
        self.closed = True

    def get_error(self) -> int:
        return self.writer.get_error()

    def set_error(self, errno: int) -> None:
        self.writer.set_error(errno)

    def _should_wait_with_autoclose(self) -> bool:
        with self.writer.close_lock:
            # Without the lock, the following race condition is possible:
            #
            # - this thread, `should_wait=True`:
            #   reader notices there is no new data to read
            # - another thread, the writer writes new data
            # - another thread, the writer closes the pipe
            # - this thread, `is_writer_closed=True`:
            #   reader notices the writer is closed
            #
            # The reader missed the new data
            writer_pos = self.writer.tell()
            should_wait = self.pos >= writer_pos
            is_writer_closed = self.writer.closed or self.writer.get_error() != 0
        if should_wait and is_writer_closed:
            self.close()
            should_wait = False
        return should_wait

    async def read(self, size: int = -1) -> bytes:
        while not self.closed:
            if self._should_wait_with_autoclose():
                await self._wait_for_writer()
                continue

            if ecode := self.writer.get_error():
                raise OSError(ecode, "Reader is in an error state")

            if size < 0:
                end_pos = len(self.writer.buffer)
            else:
                end_pos = self.pos + size
                end_pos = min(end_pos, len(self.writer.buffer))
            data = self.writer.buffer[slice(self.pos, end_pos)]
            if logger.isEnabledFor(logging.DEBUG):
                logger.debug(
                    "MemPipe.Reader.read: handle=%s, old pos=%s, new pos=%s",
                    self.handle,
                    self.pos,
                    end_pos,
                )
            self.pos = end_pos
            return data

        return b""

    async def _wait_for_writer(self) -> None:
        # See the event documentation for the workflow explanation
        lock = self.writer.queue.get_lock()
        with lock:
            if self._should_wait_with_autoclose():
                try:
                    await self.writer.queue.wait_unsafe(
                        self.writer.handle, f"MemPipe.Reader {self.handle}"
                    )
                finally:
                    lock.acquire()


class MemPipe:
    def __init__(
        self,
        writer_handle: int,
        queue: INotificationQueue,
        debug_hint: str,
        external_buffer: Optional[bytearray] = None,
    ) -> None:
        self.writer = Writer(writer_handle, queue, debug_hint, external_buffer)

    def get_writer(self) -> IAsyncWriter:
        return self.writer

    def get_reader(self, handle: int) -> IAsyncReader:
        logger.debug(
            "MemPipe.get_reader: %s for the writer %s", handle, self.writer.handle
        )
        return Reader(handle, self.writer)

    def __str__(self) -> str:
        return f"MemPipe(writer={self.writer})"


def main() -> None:
    async def write_all(lib_writer: IAsyncWriter) -> None:
        try:
            while True:
                s = await asyncio.to_thread(input)
                s = s.strip()
                if not s:
                    break
                await lib_writer.write(s.encode("utf-8"))
        except EOFError:
            pass
        finally:
            lib_writer.close()

    async def read_all(name: str, lib_reader: IAsyncReader) -> None:
        while True:
            data = await lib_reader.read(size=4)
            if len(data) == 0:
                break
            print(f"({name}): {data.decode()}")

    async def main() -> None:
        queue = NotificationQueue()
        wr = MemPipe(0, queue, "main")
        lib_writer = wr.get_writer()
        lib_reader1 = wr.get_reader(1)
        lib_reader2 = wr.get_reader(2)
        lib_reader3 = wr.get_reader(3)

        writer_task = asyncio.create_task(write_all(lib_writer))
        rt1 = asyncio.create_task(read_all("r1", lib_reader1))
        rt2 = asyncio.create_task(read_all("r2", lib_reader2))
        rt3 = asyncio.create_task(read_all("r3", lib_reader3))

        await asyncio.gather(writer_task, rt1, rt2, rt3)

    logging.basicConfig(level=logging.DEBUG)
    asyncio.run(main())


if __name__ == "__main__":
    main()
