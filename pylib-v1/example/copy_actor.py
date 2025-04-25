import logging
import os
from ailets.atyping import INodeRuntime, StdHandles


async def copy_actor(runtime: INodeRuntime) -> None:
    buffer = bytearray(1024)

    while True:
        count = await runtime.read(StdHandles.stdin, buffer, len(buffer))
        if count == 0:
            break
        if count == -1:
            raise io_errno_to_oserror(runtime.get_errno())
        data = buffer[:count]
        logging.debug(f"{runtime.get_name()}: read {count} bytes: '{data.decode()}'")
        await write_all(runtime, StdHandles.stdout, data)


def io_errno_to_oserror(ecode: int) -> OSError:
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
