import asyncio
import logging
from ailets.atyping import INodeRuntime, StdHandles
from copy_actor import write_all


async def stdin_actor(runtime: INodeRuntime) -> None:
    try:
        while True:
            s = await asyncio.to_thread(input)
            logging.debug(f"{runtime.get_name()}: read {len(s)} bytes: '{s}'")
            await write_all(runtime, StdHandles.stdout, s.encode("utf-8"))
    except EOFError:
        pass
