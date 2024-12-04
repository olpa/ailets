import asyncio
from typing import Optional


class AsyncBuffer:
    def __init__(
        self, initial_content: Optional[bytes] = None, is_closed: bool = False
    ) -> None:
        self.buffer = initial_content or b""
        self.event = asyncio.Event()
        self._is_closed = is_closed

    async def close(self) -> None:
        self._is_closed = True
        self.event.set()

    def is_closed(self) -> bool:
        return self._is_closed

    async def write(self, data: bytes) -> int:
        self.buffer += data
        self.event.set()
        return len(data)

    async def read(self, pos: int, size: int = -1) -> bytes:
        while len(self.buffer) <= pos:
            if self.is_closed():
                return b""
            await self.event.wait()
            self.event.clear()

        if size < 0:
            return self.buffer[pos:]
        end = pos + size
        if end > len(self.buffer):
            end = len(self.buffer)
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
        buffer = AsyncBuffer()
        writer_task = asyncio.create_task(writer(buffer))
        rt1 = asyncio.create_task(reader("r1", buffer))
        rt2 = asyncio.create_task(reader("r2", buffer))
        rt3 = asyncio.create_task(reader("r3", buffer))

        await asyncio.gather(writer_task, rt1, rt2, rt3)

    asyncio.run(main())