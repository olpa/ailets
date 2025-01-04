from ailets.cons.atyping import INodeRuntime
from ailets.cons.util import write_all


async def stdout(runtime: INodeRuntime) -> None:
    """Print each value to stdout and return them unchanged."""

    buffer = bytearray(1024)
    div = ""

    for i in range(runtime.n_of_streams("")):
        fd = await runtime.open_read("", i)
        while True:
            count = await runtime.read(fd, buffer, len(buffer))
            if count == 0:
                break
            if div:
                print(div, end="")
                div = ""
            print(buffer[:count].decode("utf-8"), end="", flush=True)
        await runtime.close(fd)

        div = "\n"

    fd = await runtime.open_write("")
    await write_all(runtime, fd, "ok".encode("utf-8"))
    await runtime.close(fd)
