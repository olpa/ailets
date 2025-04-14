from dataclasses import dataclass
from enum import Enum
import errno
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
    Errors,
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
        self.errno: Errors = Errors.NoError

    async def destroy(self) -> None:
        fds = list(self.open_fds.keys())
        for fd in fds:
            if self.errno != Errors.NoError:
                writer = self.open_fds[fd].writer
                if writer is not None and not writer.closed:
                    writer.set_error(errno.EPIPE)
            await self.close(fd)
            del self.open_fds[fd]

    def get_name(self) -> str:
        return self.node_name

    def get_errno(self) -> Errors:
        return self.errno

    def set_errno(self, errno: Errors) -> None:
        self.errno = errno

    async def auto_open(self, fd: StdHandles) -> None:
        opener = self.fd_openers[fd]
        node_env = self.env.for_env_pipe.get(self.node_name, {})
        if isinstance(node_env, dict):
            node_handles = node_env.get("handles", {})
            fd_str = str(int(fd))
            if isinstance(node_handles, dict) and fd_str in node_handles:
                opener = Opener(node_handles[fd_str])

        if opener == Opener.input:
            real_fd = await self.open_read("")
            self.open_fds[fd] = self.open_fds[real_fd]
            return

        if opener == Opener.output:
            real_fd = await self.open_write("")
            self.open_fds[fd] = self.open_fds[real_fd]
            return

        if opener == Opener.env:
            pipe = Piper.make_env_pipe(self.env.for_env_pipe)
            self._store_reader("env", pipe.get_reader(fd), fd)
            return

        if opener != Opener.print:
            assert False, f"Unknown opener: {opener}"
        if fd == StdHandles.stdout:
            slot_name = ""
        elif fd == StdHandles.log:
            slot_name = "log"
        elif fd == StdHandles.metrics:
            slot_name = "metrics"
        elif fd == StdHandles.trace:
            slot_name = "trace"
        else:
            assert False, f"Unknown file descriptor for Opener.print: {fd}"

        pipe = self.piper.create_pipe(self.node_name, slot_name)
        pipe = PrintWrapper(sys.stdout, pipe)
        real_fd = self._store_writer(slot_name, pipe)
        self.open_fds[fd] = self.open_fds[real_fd]
        return

    async def open_read(self, slot_name: str) -> int:
        fd = self.env.seqno.next_seqno()
        reader = MergeInputReader(self.piper, self.deps, slot_name, fd)
        self._store_reader(slot_name, reader, fd)
        return fd

    def _store_reader(self, slot_name: str, reader: IAsyncReader, fd: int) -> None:
        self.open_fds[fd] = OpenFd(
            debug_hint=f"{self.node_name}.{slot_name}",
            reader=reader,
            writer=None,
        )

    async def read(self, fd: int, buffer: bytearray, count: int) -> int:
        if fd not in self.open_fds and fd in self.fd_openers:
            await self.auto_open(StdHandles(fd))
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
        pipe = self.piper.create_pipe(self.node_name, slot_name)
        return self._store_writer(slot_name, pipe)

    def _store_writer(self, slot_name: str, pipe: IPipe) -> int:
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
            await self.auto_open(StdHandles(fd))
        assert fd in self.open_fds, f"File descriptor {fd} is not open"

        fd_obj = self.open_fds[fd]
        assert (
            fd_obj.writer is not None
        ), f"Slot {fd_obj.debug_hint} is not open for writing"
        return await fd_obj.writer.write(buffer)

    async def close(self, fd: int) -> None:
        fd_obj = self.open_fds.get(fd, None)
        if fd in self.fd_openers:
            return
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
