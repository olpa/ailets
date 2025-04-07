from dataclasses import dataclass
from typing import Dict, Optional, Sequence

from ailets.cons.input_reader import MergeInputReader

from .piper import Piper

from .node_dagops import NodeDagops
from .atyping import (
    Dependency,
    IAsyncReader,
    IAsyncWriter,
    IEnvironment,
    INodeDagops,
    INodeRuntime,
    IPipe,
)


@dataclass
class OpenFd:
    debug_hint: str
    reader: Optional[IAsyncReader]
    writer: Optional[IAsyncWriter]


class NodeRuntime(INodeRuntime):
    def __init__(
        self,
        env: IEnvironment,
        node_name: str,
        deps: Sequence[Dependency],
    ):
        self.env = env
        self.piper = env.piper
        self.node_name = node_name
        self.deps = deps
        self.open_fds: Dict[int, OpenFd] = {}
        self.cached_dagops: Optional[INodeDagops] = None

    def get_name(self) -> str:
        return self.node_name

    def n_of_inputs(self, slot_name: str) -> int:
        return 1

    async def open_read(self, slot_name: str, index: int) -> int:
        fd = self.env.seqno.next_seqno()

        reader: IAsyncReader
        if slot_name == "env":
            pipe = Piper.make_env_pipe(self.env.for_env_pipe)
            reader = pipe.get_reader(fd)
        else:
            reader = MergeInputReader(self.piper, self.deps, slot_name, fd)

        self.open_fds[fd] = OpenFd(
            debug_hint=f"{self.node_name}.{slot_name}[{index}]",
            reader=reader,
            writer=None,
        )
        return fd

    async def read(self, fd: int, buffer: bytearray, count: int) -> int:
        assert fd in self.open_fds, f"File descriptor {fd} is not open"
        fd_obj = self.open_fds[fd]
        assert (
            fd_obj.reader is not None
        ), f"Slot {fd_obj.debug_hint} is not open for reading"
        read_bytes = await fd_obj.reader.read(count)
        n_bytes = len(read_bytes)
        buffer[:n_bytes] = read_bytes
        return n_bytes

    async def open_write(self, slot_name: str) -> int:
        pipe: IPipe
        if slot_name in ("log", "metrics", "trace"):
            pipe = Piper.make_log_pipe()
        else:
            pipe = self.piper.create_pipe(self.node_name, slot_name)
        fd = self.env.seqno.next_seqno()
        writer = pipe.get_writer()
        self.open_fds[fd] = OpenFd(
            debug_hint=f"{self.node_name}.{slot_name}",
            reader=None,
            writer=writer,
        )
        return fd

    async def write(self, fd: int, buffer: bytes, count: int) -> int:
        assert fd in self.open_fds, f"File descriptor {fd} is not open"
        fd_obj = self.open_fds[fd]
        assert (
            fd_obj.writer is not None
        ), f"Slot {fd_obj.debug_hint} is not open for writing"
        return await fd_obj.writer.write(buffer)

    async def close(self, fd: int) -> None:
        assert fd in self.open_fds, f"File descriptor {fd} is not open"
        fd_obj = self.open_fds.pop(fd)
        if fd_obj.reader is not None:
            fd_obj.reader.close()
        if fd_obj.writer is not None:
            fd_obj.writer.close()

    def dagops(self) -> INodeDagops:
        if self.cached_dagops is None:
            self.cached_dagops = NodeDagops(self.env, self)
        return self.cached_dagops

    async def read_dir(self, dir_name: str) -> Sequence[str]:
        return self.env.kv.listdir(dir_name)

    def get_next_name(self, base_name: str) -> str:
        return self.env.dagops.get_next_name(base_name)
