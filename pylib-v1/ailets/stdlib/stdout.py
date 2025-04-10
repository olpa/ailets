from ailets.cons.atyping import INodeRuntime, StdHandles
from ailets.cons.util import write_all


async def stdout(runtime: INodeRuntime) -> None:
    """Print each value to stdout and return them unchanged."""

    buffer = bytearray(1024)
    fd = await runtime.open_read("")
    while True:
        count = await runtime.read(fd, buffer, len(buffer))
        if count == 0:
            break
        print(buffer[:count].decode("utf-8"), end="", flush=True)

    await write_all(runtime, StdHandles.stdout, "ok".encode("utf-8"))
