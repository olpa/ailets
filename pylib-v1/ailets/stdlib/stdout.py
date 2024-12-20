from ailets.cons.atyping import INodeRuntime
from ailets.cons.util import read_all, write_all


async def stdout(runtime: INodeRuntime) -> None:
    """Print each value to stdout and return them unchanged."""

    for i in range(runtime.n_of_streams("")):
        fd = await runtime.open_read("", i)
        value = (await read_all(runtime, fd)).decode("utf-8")
        await runtime.close(fd)

        if value == "":
            continue
        print(value)

    fd = await runtime.open_write("")
    await write_all(runtime, fd, "ok".encode("utf-8"))
    await runtime.close(fd)
