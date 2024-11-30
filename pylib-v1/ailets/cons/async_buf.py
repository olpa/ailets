import asyncio


class AsyncBuffer:
    def __init__(self):
        self.buffer = b""
        self.event = asyncio.Event()
        self.is_closed = False

    def close(self):
        self.is_closed = True
        self.event.set()

    async def write(self, data):
        self.buffer += data
        self.event.set()

    async def read(self, pos, size=-1):
        while len(self.buffer) <= pos:
            if self.is_closed:
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

    async def writer(buffer):
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
            buffer.close()

    async def reader(name, buffer):
        pos = 0
        while True:
            data = await buffer.read(pos, size=4)
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
