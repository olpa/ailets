import json
import sys  # FIXME
from typing import Any, AsyncGenerator, Dict, Optional, Sequence

from ailets.atyping import Dependency, IAsyncReader, INodeRuntime, IPiper, IPipe, StdHandles, ILiveDependencies
from ailets.cons.util import io_errno_to_oserror


def _get_pipes(
    piper: IPiper, deps: Sequence[Dependency], slot_name: str
) -> Sequence[IPipe]:
    slot_deps = [dep for dep in deps if dep.name == slot_name]
    pipes = []
    for dep in slot_deps:
        try:
            pipe = piper.get_future_pipe(dep.source, dep.slot)
        except KeyError:
            raise RuntimeError(f"Expected pipe for dependency '{dep.source}:{dep.slot}' in slot '{slot_name}' does not exist in VFS")
        pipes.append(pipe)
    return pipes


class MergeInputReader(IAsyncReader):
    def __init__(
        self,
        piper: IPiper,
        live_deps: ILiveDependencies,
        slot_name: str,
        read_handle: int,
    ):
        self.piper = piper
        self.live_deps = live_deps
        self.slot_name = slot_name
        self.read_handle = read_handle
        self.index = -1
        self.current_reader: Optional[IAsyncReader] = None
        self.closed = False
        self.errno = 0

    def close(self) -> None:
        self.closed = True

    def set_error(self, errno: int) -> None:
        self.errno = errno
        if self.current_reader is not None:
            self.current_reader.set_error(errno)

    async def read(self, size: int) -> bytes:
        if self.closed:
            return b""
        if self.errno != 0:
            raise OSError(self.errno, "Reader is in an error state")

        if self.current_reader is not None:
            if self.current_reader.closed:
                self.current_reader = None

        if self.current_reader is not None:
            bytes_read = await self.current_reader.read(size)
            if len(bytes_read) > 0:
                return bytes_read
            self.current_reader.close()
            self.current_reader = None

        current_deps = self.live_deps.get_dependencies()
        pipes = _get_pipes(self.piper, current_deps, self.slot_name)
        self.index += 1

        print(f"!!!debug MergeInputReader for '{self.slot_name}', try advance: index={self.index}, N pipes={len(pipes)}, deps: {current_deps}", file=sys.stderr)  # FIXME

        # Open an attachment from the kv
        if not len(pipes) and self.index == 0:
            if "/" in self.slot_name:
                kv_base = "/"
            elif self.slot_name.startswith("value."):
                kv_base = ""
            else:
                kv_base = None
            if kv_base is not None:
                try:
                    pipe = self.piper.get_existing_pipe(kv_base, self.slot_name)
                    pipes = [pipe]
                except KeyError:
                    pass

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
        if count == -1:
            raise io_errno_to_oserror(runtime.get_errno())
        result.extend(buffer[:count])
    return bytes(result)


async def iter_input_objects(
    runtime: INodeRuntime,
    slot_name: str | int,
    sse_tokens: Sequence[str] = (),
) -> AsyncGenerator[dict[str, Any], None]:
    """Iterate over all slots. Each slot contains JSON objects,
    either as a JSON array or as individual objects without separation."""
    if isinstance(slot_name, int):
        fd = slot_name
    else:
        fd = await runtime.open_read(slot_name)
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
            yield obj

        except json.JSONDecodeError:
            raise ValueError(
                f"Failed to decode JSON at position {pos}: " f"{sbuf[pos:pos+20]!r}..."
            )


async def read_env_pipe(runtime: INodeRuntime) -> Dict[str, Any]:
    env: dict[str, Any] = {}
    async for params in iter_input_objects(runtime, StdHandles.env):
        env.update(params)
    return env
