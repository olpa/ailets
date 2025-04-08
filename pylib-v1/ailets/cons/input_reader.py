import json
from typing import Any, AsyncGenerator, Optional, Sequence

from .atyping import Dependency, IAsyncReader, INodeRuntime, IPiper, IPipe


def _get_pipes(
    piper: IPiper, deps: Sequence[Dependency], slot_name: str
) -> Sequence[IPipe]:
    slot_deps = [dep for dep in deps if dep.name == slot_name]
    pipes = []
    for dep in slot_deps:
        try:
            pipe = piper.get_existing_pipe(dep.source, dep.slot)
        except KeyError:
            continue
        pipes.append(pipe)
    return pipes


class MergeInputReader(IAsyncReader):
    def __init__(
        self,
        piper: IPiper,
        deps: Sequence[Dependency],
        slot_name: str,
        read_handle: int,
    ):
        self.piper = piper
        self.deps = deps
        self.slot_name = slot_name
        self.read_handle = read_handle
        self.index = -1
        self.current_reader: Optional[IAsyncReader] = None
        self.closed = False

    def close(self) -> None:
        self.closed = True

    async def read(self, size: int) -> bytes:
        if self.closed:
            return b""

        if self.current_reader is not None:
            if self.current_reader.closed:
                self.current_reader = None

        if self.current_reader is not None:
            bytes_read = await self.current_reader.read(size)
            if len(bytes_read) > 0:
                return bytes_read
            self.current_reader.close()
            self.current_reader = None

        pipes = _get_pipes(self.piper, self.deps, self.slot_name)
        self.index += 1
        if self.index >= len(pipes):
            self.closed = True
            return b""

        self.current_reader = pipes[self.index].get_reader(self.read_handle)
        return await self.read(size)


async def read_all(runtime: INodeRuntime, fd: int) -> bytes:
    buffer = bytearray(1024)
    result = bytearray()
    while True:
        count = await runtime.read(fd, buffer, len(buffer))
        if count == 0:
            break
        result.extend(buffer[:count])
    return bytes(result)


async def iter_input_objects(
    runtime: INodeRuntime,
    slot_name: str,
    sse_tokens: Sequence[str] = (),
) -> AsyncGenerator[dict[str, Any], None]:
    """Iterate over all slots. Each slot contains JSON objects,
    either as a JSON array or as individual objects without separation."""
    fd = await runtime.open_read(slot_name, 0)
    buffer = await read_all(runtime, fd)
    await runtime.close(fd)

    if len(buffer) == 0:
        return

    sbuf = buffer.decode("utf-8")

    decoder = json.JSONDecoder()

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
        if sbuf[pos] != "{" and sbuf[pos] != "[":
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

            if isinstance(obj, list):
                for item in obj:
                    yield item
            else:
                yield obj

        except json.JSONDecodeError:
            raise ValueError(
                f"Failed to decode JSON at position {pos}: " f"{sbuf[pos:pos+20]!r}..."
            )
