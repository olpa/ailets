from ailets.cons.atyping import INodeRuntime
from ailets.cons.util import write_all


async def tmptmp(runtime: INodeRuntime) -> None:
    """Print each value to stdout and return them unchanged."""

    fd = await runtime.open_write("")
    await write_all(runtime, fd, "text1\n\n".encode("utf-8"))
    await write_all(runtime, fd, "text2\n\n".encode("utf-8"))
    await write_all(runtime, fd, "text3\n\n".encode("utf-8"))
    await write_all(runtime, fd, "text4\n\n".encode("utf-8"))
    await write_all(runtime, fd, "text5\n\n".encode("utf-8"))
    await runtime.close(fd)
