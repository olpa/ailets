import os
from ailets.atyping import INodeRuntime, StdHandles
from ailets.cons.util import write_all


async def call(runtime: INodeRuntime) -> None:
    """Call the get_user_name tool."""
    value = os.environ["USER"]

    await write_all(runtime, StdHandles.stdout, value.encode("utf-8"))
