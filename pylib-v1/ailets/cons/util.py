import json
from typing import Iterator
from .typing import INodeRuntime


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


def iter_streams_objects(runtime: INodeRuntime) -> Iterator[dict]:
    """Iterate over all streams. Each stream contains JSON objects,
    either as a JSON array or as individual objects without separation."""
    # `n_of_streams` can change with time, therefore don't use `range`
    i = 0
    while i < runtime.n_of_streams(None):
        stream = runtime.open_read(None, i)
        decoder = json.JSONDecoder()
        buffer = stream.read()

        if buffer[0] == "[":
            array = json.loads(buffer)
            yield from array
            return

        pos = 0
        while pos < len(buffer):
            try:
                obj, pos = decoder.raw_decode(buffer[pos:])
                yield obj
            except json.JSONDecodeError:
                break
        i += 1
