import asyncio
import logging
from typing import Callable, Optional

logger = logging.getLogger("ailets.io")


class AsyncBuffer:
    def __init__(
        self,
        initial_content: Optional[bytes],
        is_closed: bool,
        on_write_started: Callable[[], None],
        debug_hint: Optional[str] = None,
    ) -> None:
        self.buffer = initial_content or b""
        self.event = asyncio.Event()
        self._is_closed = is_closed
        self.on_write_started = on_write_started
        self.debug_hint = debug_hint

    async def close(self) -> None:
        self._is_closed = True
        self.event.set()
        logger.debug(
            "Buffer closed%s", f" ({self.debug_hint})" if self.debug_hint else ""
        )

    def is_closed(self) -> bool:
        return self._is_closed

    async def write(self, data: bytes) -> int:
        old_pos = len(self.buffer)
        self.buffer += data
        new_pos = len(self.buffer)
        self.event.set()
        logger.debug(
            "Buffer write%s: pos %d->%d",
            f" ({self.debug_hint})" if self.debug_hint else "",
            old_pos,
            new_pos,
        )
        if old_pos == 0 and new_pos > 0:
            self.on_write_started()
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

    asyncio.run(main())
