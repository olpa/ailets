import errno
import os
from typing import Any, Literal

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


def io_errno_to_oserror(ecode: int) -> OSError:
    if ecode == -1:
        ecode = errno.EPIPE
    msg = "unknown error"
    try:
        msg = os.strerror(ecode)
    except ValueError:
        pass
    return OSError(ecode, msg)


async def write_all(runtime: INodeRuntime, fd: int, data: bytes) -> None:
    pos = 0
    while pos < len(data):
        count = await runtime.write(fd, data[pos:], len(data) - pos)
        if count == -1:
            raise io_errno_to_oserror(runtime.get_errno())
        pos += count


async def log(
    runtime: INodeRuntime, level: Literal["info", "warn", "error"], *message: Any
) -> None:
    message_str = " ".join(map(str, message))
    log_str = f"{runtime.get_name()}: {level} {message_str}\n"

    await write_all(runtime, StdHandles.log, log_str.encode("utf-8"))
