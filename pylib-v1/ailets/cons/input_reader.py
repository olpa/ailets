from typing import Optional, Sequence

from .atyping import Dependency, IAsyncReader, IPiper, IPipe


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
        if self.index > len(pipes):
            self.closed = True
            return b""

        self.index += 1
        self.current_reader = pipes[self.index].get_reader(self.read_handle)
        return await self.read(size)
