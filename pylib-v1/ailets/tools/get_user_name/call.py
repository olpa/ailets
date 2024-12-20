import os
from ailets.cons.atyping import INodeRuntime
from ailets.cons.util import write_all


async def call(runtime: INodeRuntime) -> None:
    """Call the get_user_name tool."""
    value = os.environ["USER"]

    fd = await runtime.open_write("")
    await write_all(runtime, fd, value.encode("utf-8"))
    await runtime.close(fd)
