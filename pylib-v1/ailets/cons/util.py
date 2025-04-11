from typing import Any, Dict, Literal

from ailets.cons.input_reader import iter_input_objects
from .atyping import INodeRuntime, StdHandles


def to_basename(name: str) -> str:
    """Return the base name of a node, stripping off any numeric suffix.

    Args:
        name: The full name of the node

    Returns:
        The base name of the node without the numeric suffix
    """
    if "." in name and name.split(".")[-1].isdigit():
        return ".".join(name.split(".")[:-1])
    return name


async def write_all(runtime: INodeRuntime, fd: int, data: bytes) -> None:
    pos = 0
    while pos < len(data):
        count = await runtime.write(fd, data[pos:], len(data) - pos)
        pos += count


async def read_env_pipe(runtime: INodeRuntime) -> Dict[str, Any]:
    env: dict[str, Any] = {}
    async for params in iter_input_objects(runtime, StdHandles.env):
        env.update(params)
    return env


async def log(
    runtime: INodeRuntime, level: Literal["info", "warn", "error"], *message: Any
) -> None:
    message_str = " ".join(map(str, message))
    log_str = f"{runtime.get_name()}: {level} {message_str}\n"

    await write_all(runtime, StdHandles.log, log_str.encode("utf-8"))
