from ailets.cons.atyping import INodeRuntime, StdHandles
from ailets.cons.util import write_all


async def stdout(runtime: INodeRuntime) -> None:
    """Print each value to stdout and return them unchanged."""

    buffer = bytearray(1024)
    while True:
        count = await runtime.read(StdHandles.stdin, buffer, len(buffer))
        if count == 0:
            break
        print(buffer[:count].decode("utf-8"), end="", flush=True)

    await write_all(runtime, StdHandles.stdout, "ok".encode("utf-8"))
