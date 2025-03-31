import asyncio
import logging

from .atyping import IAsyncReader, IAsyncWriter
from .notification_queue import INotificationQueue, NotificationQueue

logger = logging.getLogger("ailets.io")


class Writer(IAsyncWriter):
    def __init__(self, handle: int, queue: INotificationQueue, debug_hint: str) -> None:
        super().__init__()
        self.buffer = bytearray()
        self.handle = handle
        self.queue = queue
        self.debug_hint = debug_hint
        self.closed = False
        self.queue.whitelist(handle, f"BytesWR.Writer {debug_hint}")

    def __str__(self) -> str:
        return (
            f"BytesWR.Writer(handle={self.handle}, "
            f"closed={self.closed}, "
            f"tell={self.tell()}, "
            f"hint={self.debug_hint})"
        )

    async def write(self, data: bytes) -> int:
        return self.write_sync(data)

    def write_sync(self, data: bytes) -> int:
        if self.closed:
            raise ValueError("Writer is closed")
        if len(data) == 0:
            return 0
        self.buffer.extend(data)
        self.queue.notify(self.handle, len(data))
        return len(data)

    def tell(self) -> int:
        return len(self.buffer)

    def close(self) -> None:
        self.closed = True
        self.queue.unlist(self.handle)
        self.queue.notify(self.handle, -1)


class Reader(IAsyncReader):
    def __init__(self, handle: int, writer: Writer) -> None:
        super().__init__()
        self.handle = handle
        self.writer = writer
        self.pos = 0
        self.closed = False

    def close(self) -> None:
        self.closed = True

    def _should_wait_with_autoclose(self) -> bool:
        should_wait = self.pos >= self.writer.tell()
        if should_wait and self.writer.closed:
            self.close()
            should_wait = False
        return should_wait

    async def read(self, size: int = -1) -> bytes:
        while not self.closed:
            if self._should_wait_with_autoclose():
                await self._wait_for_writer()
                continue

            if size < 0:
                end_pos = len(self.writer.buffer)
            else:
                end_pos = self.pos + size
            data = self.writer.buffer[slice(self.pos, end_pos)]
            self.pos = end_pos
            return data

        return b""

    async def _wait_for_writer(self) -> None:
        # See the event documentation for the workflow explanation
        lock = self.writer.queue.get_lock()
        with lock:
            if self._should_wait_with_autoclose():
                await self.writer.queue.wait_unsafe(
                    self.writer.handle, f"BytesWR.Reader {self.handle}"
                )
                lock.acquire()
        if self.writer.closed:
            self.close()


class BytesWR:
    def __init__(
        self, writer_handle: int, queue: INotificationQueue, debug_hint: str
    ) -> None:
        self.writer = Writer(writer_handle, queue, debug_hint)

    def get_writer(self) -> IAsyncWriter:
        return self.writer

    def get_reader(self, handle: int) -> IAsyncReader:
        logger.debug(
            "BytesWR.get_reader: %s for the writer %s", handle, self.writer.handle
        )
        return Reader(handle, self.writer)

    def __str__(self) -> str:
        return f"BytesWR(writer={self.writer})"


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
        wr = BytesWR(0, queue, "main")
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
