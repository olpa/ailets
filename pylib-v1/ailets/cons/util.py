import errno
from io import BytesIO
import os
from typing import Any, Awaitable, Callable, Literal, Optional, Generator, BinaryIO
from contextlib import contextmanager

from ailets.atyping import IKVBuffers, INodeRuntime, StdHandles


def get_path(node_name: str, slot_name: Optional[str]) -> str:
    if not slot_name:
        return node_name
    if "/" in slot_name:
        return slot_name
    return f"{node_name}-{slot_name}"


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


async def save_file(
    vfs: Optional[IKVBuffers],
    path: str,
    with_open_stream: Callable[[BytesIO], Awaitable[None]],
) -> None:
    if vfs is None:
        with open(path, "wb") as h:
            h2: BytesIO = h  # type: ignore[assignment]
            await with_open_stream(h2)
        return

    bio = BytesIO()
    await with_open_stream(bio)
    bio.flush()
    b = bio.getvalue()

    bufref = vfs.open(path, "write")
    buf = bufref.borrow_mut_buffer()
    buf[:] = b
    vfs.flush(path)


@contextmanager
def open_file(vfs: Optional[IKVBuffers], path: str) -> Generator[BinaryIO, None, None]:
    if vfs is None:
        with open(path, "rb") as h:
            yield h
        return

    try:
        bufref = vfs.open(path, "read")
    except KeyError:
        with open(path, "rb") as h:
            yield h
        return

    buf = bufref.borrow_mut_buffer()
    bio = BytesIO(buf)
    try:
        yield bio
    finally:
        bio.close()
