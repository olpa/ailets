import json
from typing import Any, Dict, Iterator, Literal, Optional
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


def read_all(runtime: INodeRuntime, fd: int) -> bytes:
    buffer = bytearray(1024)
    result = bytearray()
    while True:
        count = runtime.read(fd, buffer, len(buffer))
        if count == 0:
            break
        result.extend(buffer[:count])
    return bytes(result)


def write_all(runtime: INodeRuntime, fd: int, data: bytes) -> None:
    pos = 0
    while pos < len(data):
        count = runtime.write(fd, data[pos:], len(data) - pos)
        pos += count


def iter_streams_objects(
    runtime: INodeRuntime, stream_name: Optional[str]
) -> Iterator[dict[str, Any]]:
    """Iterate over all streams. Each stream contains JSON objects,
    either as a JSON array or as individual objects without separation."""
    # `n_of_streams` can change with time, therefore don't use `range`
    i = 0
    while i < runtime.n_of_streams(stream_name):
        i += 1

        fd = runtime.open_read(stream_name, i - 1)
        buffer = read_all(runtime, fd)
        runtime.close(fd)
        sbuf = buffer.decode("utf-8")

        decoder = json.JSONDecoder()

        if buffer[0] == ord("["):
            array = json.loads(buffer)
            yield from array
            continue

        pos = 0
        while pos < len(sbuf):
            try:
                obj, obj_len = decoder.raw_decode(sbuf[pos:])
                pos += obj_len
                yield obj

            except json.JSONDecodeError:
                raise ValueError(
                    f"Failed to decode JSON at position {pos}: "
                    f"{sbuf[pos:pos+20]!r}..."
                )


def read_env_stream(runtime: INodeRuntime) -> Dict[str, Any]:
    return {
        k: v
        for params in iter_streams_objects(runtime, "env")
        for k, v in params.items()
    }


def log(
    runtime: INodeRuntime, level: Literal["info", "warn", "error"], *message: Any
) -> None:
    message_str = " ".join(map(str, message))
    log_str = f"{runtime.get_name()}: {level} {message_str}\n"

    fd = runtime.open_write("log")
    write_all(runtime, fd, log_str.encode("utf-8"))
    runtime.close(fd)
