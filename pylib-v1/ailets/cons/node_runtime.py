from dataclasses import dataclass
from enum import Enum
import sys
from typing import Dict, Optional, Sequence

from ailets.cons.input_reader import MergeInputReader

from .piper import Piper, PrintWrapper

from .node_dagops import NodeDagops
from .atyping import (
    Dependency,
    IAsyncReader,
    IAsyncWriter,
    IEnvironment,
    INodeDagops,
    INodeRuntime,
    IPipe,
    StdHandles,
)


@dataclass
class OpenFd:
    debug_hint: str
    reader: Optional[IAsyncReader]
    writer: Optional[IAsyncWriter]


class Opener(Enum):
    input = "input"
    output = "output"
    print = "print"
    env = "env"


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
        self.fd_openers: Dict[StdHandles, Opener] = {
            StdHandles.stdin: Opener.input,
            StdHandles.stdout: Opener.output,
            StdHandles.log: Opener.print,
            StdHandles.env: Opener.env,
            StdHandles.metrics: Opener.print,
            StdHandles.trace: Opener.print,
        }

    async def destroy(self) -> None:
        fds = list(self.open_fds.keys())
        for fd in fds:
            await self.close(fd)
            del self.open_fds[fd]

    def get_name(self) -> str:
        return self.node_name

    async def open_read(self, slot_name: str) -> int:
        fd = self.env.seqno.next_seqno()

        reader: IAsyncReader
        if slot_name == "env":
            pipe = Piper.make_env_pipe(self.env.for_env_pipe)
            reader = pipe.get_reader(fd)
        else:
            reader = MergeInputReader(self.piper, self.deps, slot_name, fd)

        self.open_fds[fd] = OpenFd(
            debug_hint=f"{self.node_name}.{slot_name}",
            reader=reader,
            writer=None,
        )
        return fd

    async def auto_open(self, fd: int, opener: Opener) -> None:
        if opener == Opener.input:
            real_fd = await self.open_read("")
            self.open_fds[fd] = self.open_fds[real_fd]
            return

        if opener == Opener.output:
            real_fd = await self.open_write("")
            self.open_fds[fd] = self.open_fds[real_fd]
            return

        if opener == Opener.env:
            real_fd = await self.open_read("env")
            self.open_fds[fd] = self.open_fds[real_fd]
            return

        if opener != Opener.print:
            assert False, f"Unknown opener: {opener}"
        if fd == StdHandles.log:
            slot_name = "log"
        elif fd == StdHandles.metrics:
            slot_name = "metrics"
        elif fd == StdHandles.trace:
            slot_name = "trace"
        else:
            assert False, f"Unknown file descriptor for Opener.print: {fd}"

        pipe = self.piper.create_pipe(self.node_name, slot_name)
        pipe = PrintWrapper(sys.stdout, pipe)
        self.open_fds[fd] = OpenFd(
            debug_hint=f"{self.node_name}.{slot_name}",
            reader=None,
            writer=pipe.get_writer(),
        )

    async def read(self, fd: int, buffer: bytearray, count: int) -> int:
        if fd not in self.open_fds and fd in self.fd_openers:
            std_fd: StdHandles = StdHandles(fd)
            await self.auto_open(std_fd, self.fd_openers[std_fd])
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
        if fd not in self.open_fds and fd in self.fd_openers:
            std_fd: StdHandles = StdHandles(fd)
            await self.auto_open(std_fd, self.fd_openers[std_fd])
        assert fd in self.open_fds, f"File descriptor {fd} is not open"

        fd_obj = self.open_fds[fd]
        assert (
            fd_obj.writer is not None
        ), f"Slot {fd_obj.debug_hint} is not open for writing"
        return await fd_obj.writer.write(buffer)

    async def close(self, fd: int) -> None:
        fd_obj = self.open_fds.get(fd, None)
        assert fd_obj is not None, f"File descriptor {fd} is not open"
        if fd_obj.reader is not None:
            if not fd_obj.reader.closed:
                fd_obj.reader.close()
        if fd_obj.writer is not None:
            if not fd_obj.writer.closed:
                fd_obj.writer.close()

    def dagops(self) -> INodeDagops:
        if self.cached_dagops is None:
            self.cached_dagops = NodeDagops(self.env, self)
        return self.cached_dagops

    def get_next_name(self, base_name: str) -> str:
        return self.env.dagops.get_next_name(base_name)
