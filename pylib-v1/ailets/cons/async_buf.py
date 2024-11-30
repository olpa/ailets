import asyncio


class AsyncBuffer:
    def __init__(self):
        self.buffer = b""
        self.event = asyncio.Event()

    async def write(self, data):
        self.buffer += data
        self.event.set()

    async def read(self, pos, size=-1):
        await self.event.wait()
        self.event.clear()

        if len(self.buffer) <= pos:
            return b""

        if size < 0:
            return self.buffer[pos:]
        end = pos + size
        if end > len(self.buffer):
            end = len(self.buffer)
        return self.buffer[pos:end]


if __name__ == "__main__":
    import sys

    async def writer(buffer):
        for line in sys.stdin:
            await buffer.write(line.strip().encode("utf-8"))

    async def reader(name, buffer):
        pos = 0
        while True:
            data = await buffer.read(pos)
            size = len(data)
            pos += size

            if size == 0:
                break
            print(f"({name}): {data.decode()}")

    async def main():
        buffer = AsyncBuffer()
        writer_task = asyncio.create_task(writer(buffer))
        rt1 = asyncio.create_task(reader("r1", buffer))
        rt2 = asyncio.create_task(reader("r2", buffer))
        rt3 = asyncio.create_task(reader("r3", buffer))

        await asyncio.gather(writer_task, rt1, rt2, rt3)

    asyncio.run(main())
