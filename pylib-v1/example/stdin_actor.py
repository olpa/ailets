import asyncio
from ailets.atyping import INodeRuntime, StdHandles
from copy_actor import write_all


async def stdin_actor(runtime: INodeRuntime) -> None:
    try:
        while True:
            s = await asyncio.to_thread(input)
            s = s.strip()
            await write_all(runtime, StdHandles.stdout, s.encode("utf-8"))
    except EOFError:
        pass
