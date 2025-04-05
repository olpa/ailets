import json
from typing import Any, AsyncGenerator, Dict, Literal, Sequence
from .atyping import INodeRuntime


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


async def read_all(runtime: INodeRuntime, fd: int) -> bytes:
    buffer = bytearray(1024)
    result = bytearray()
    while True:
        count = await runtime.read(fd, buffer, len(buffer))
        if count == 0:
            break
        result.extend(buffer[:count])
    return bytes(result)


async def write_all(runtime: INodeRuntime, fd: int, data: bytes) -> None:
    pos = 0
    while pos < len(data):
        count = await runtime.write(fd, data[pos:], len(data) - pos)
        pos += count


async def iter_input_objects(
    runtime: INodeRuntime,
    slot_name: str,
    sse_tokens: Sequence[str] = (),
) -> AsyncGenerator[dict[str, Any], None]:
    """Iterate over all slots. Each slot contains JSON objects,
    either as a JSON array or as individual objects without separation."""
    # `n_of_inputs` can change with time, therefore don't use `range`
    i = 0
    while i < runtime.n_of_inputs(slot_name):
        i += 1

        fd = await runtime.open_read(slot_name, i - 1)
        buffer = await read_all(runtime, fd)
        await runtime.close(fd)

        if len(buffer) == 0:
            continue

        sbuf = buffer.decode("utf-8")

        decoder = json.JSONDecoder()

        if buffer[0] == ord("["):
            array = json.loads(buffer)
            for item in array:
                yield item
            continue

        pos = 0
        while pos < len(sbuf):
            #
            # Skip whitespace and SSE tokens
            #
            while pos < len(sbuf) and sbuf[pos].isspace():
                pos += 1
            if pos >= len(sbuf):
                break

            skipped_sse_tokens = False
            if sbuf[pos] != "{":
                for token in sse_tokens:
                    if sbuf[pos:].startswith(token):
                        skipped_sse_tokens = True
                        pos += len(token)
                        break

            if skipped_sse_tokens:
                continue

            #
            # Parse JSON object
            #
            try:
                obj, obj_len = decoder.raw_decode(sbuf[pos:])
                pos += obj_len
                yield obj

            except json.JSONDecodeError:
                raise ValueError(
                    f"Failed to decode JSON at position {pos}: "
                    f"{sbuf[pos:pos+20]!r}..."
                )


async def read_env_pipe(runtime: INodeRuntime) -> Dict[str, Any]:
    env: dict[str, Any] = {}
    async for params in iter_input_objects(runtime, "env"):
        env.update(params)
    return env


async def log(
    runtime: INodeRuntime, level: Literal["info", "warn", "error"], *message: Any
) -> None:
    message_str = " ".join(map(str, message))
    log_str = f"{runtime.get_name()}: {level} {message_str}\n"

    fd = await runtime.open_write("log")
    await write_all(runtime, fd, log_str.encode("utf-8"))
    await runtime.close(fd)
