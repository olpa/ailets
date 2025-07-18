import errno
import logging
from dataclasses import dataclass
from enum import Enum
import sys
from typing import Dict, Optional, Sequence

from ailets.cons.util import get_path
from ailets.io.input_reader import MergeInputReader

from ailets.io.piper import Piper, PrintWrapper

from ailets.actor_runtime.node_dagops import NodeDagops
from ailets.atyping import (
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
        self.errno: int = 0
        self.logger = logging.getLogger(f"ailets.actor.{node_name}")

    async def destroy(self) -> None:
        fds = list(self.open_fds.keys())
        for fd in fds:
            if self.errno != 0:
                reader = self.open_fds[fd].reader
                if reader is not None and not reader.closed:
                    reader.set_error(errno.EPIPE)
                writer = self.open_fds[fd].writer
                if writer is not None and not writer.closed:
                    writer.set_error(errno.EPIPE)
            await self.close(fd)
            del self.open_fds[fd]

    def get_name(self) -> str:
        return self.node_name

    def get_errno(self) -> int:
        return self.errno

    def set_errno(self, errno: int) -> None:
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
        real_fd = self.private_store_writer(slot_name, pipe)
        self.open_fds[fd] = self.open_fds[real_fd]
        return
    
    def handle_error(self, prefix: str, e: Exception) -> None:
        if self.logger.isEnabledFor(logging.DEBUG):
            self.logger.error(f"{prefix} error", exc_info=e)
        elif isinstance(e, OSError):
            # Silent: OSError is expected and to be handled by the caller
            self.set_errno(e.errno)
        else:
            self.logger.error(f"{prefix} error: {e}")
        if not self.errno:
            self.set_errno(-1)

    async def open_read(self, slot_name: str) -> int:
        try:
            fd = self.env.seqno.next_seqno()
            reader = MergeInputReader(self.piper, self.deps, slot_name, fd)
            self._store_reader(slot_name, reader, fd)
            return fd
        except Exception as e:
            self.handle_error("open_read", e)
            return -1

    def _store_reader(self, slot_name: str, reader: IAsyncReader, fd: int) -> None:
        self.open_fds[fd] = OpenFd(
            debug_hint=f"{self.node_name}.{slot_name}",
            reader=reader,
            writer=None,
        )

    async def read(self, fd: int, buffer: bytearray, count: int) -> int:
        try:
            if fd not in self.open_fds and fd in self.fd_openers:
                await self.auto_open(StdHandles(fd))
            if fd not in self.open_fds:
                self.set_errno(errno.EBADF)
                self.logger.debug(f"File descriptor {fd} is not open")
                return -1

            fd_obj = self.open_fds[fd]
            if fd_obj.reader is None:
                self.set_errno(errno.EBADF)
                self.logger.debug(f"Slot {fd_obj.debug_hint} is not open for reading")
                return -1

            read_bytes = await fd_obj.reader.read(count)
            n_bytes = len(read_bytes)
            buffer[:n_bytes] = read_bytes
            return n_bytes

        except Exception as e:
            self.handle_error("read", e)
            return -1

    async def open_write(self, slot_name: str) -> int:
        try:
            pipe = self.piper.create_pipe(self.node_name, slot_name)
            return self.private_store_writer(slot_name, pipe)
        except Exception as e:
            self.handle_error("open_write", e)
            return -1

    def private_store_writer(self, slot_name: str, pipe: IPipe) -> int:
        fd = self.env.seqno.next_seqno()
        writer = pipe.get_writer()
        self.open_fds[fd] = OpenFd(
            debug_hint=f"{self.node_name}.{slot_name}",
            reader=None,
            writer=writer,
        )
        return fd

    async def write(self, fd: int, buffer: bytes, count: int) -> int:
        try:
            if fd not in self.open_fds and fd in self.fd_openers:
                await self.auto_open(StdHandles(fd))
            if fd not in self.open_fds:
                self.set_errno(errno.EBADF)
                self.logger.debug(f"File descriptor {fd} is not open")
                return -1

            fd_obj = self.open_fds[fd]
            if fd_obj.writer is None:
                self.set_errno(errno.EBADF)
                self.logger.debug(f"Slot {fd_obj.debug_hint} is not open for writing")
                return -1

            return await fd_obj.writer.write(buffer)

        except Exception as e:
            self.handle_error("write", e)
            return -1

    async def close(self, fd: int) -> int:
        try:
            fd_obj = self.open_fds.get(fd, None)
            if fd in self.fd_openers:
                return 0
            if fd_obj is None:
                self.set_errno(errno.EBADF)
                self.logger.debug(f"File descriptor {fd} is not open")
                return -1

            if fd_obj.reader is not None:
                if not fd_obj.reader.closed:
                    fd_obj.reader.close()
            if fd_obj.writer is not None:
                if not fd_obj.writer.closed:
                    fd_obj.writer.close()
                    path = get_path(self.node_name, "")
                    self.env.kv.flush(path)
            return 0
        except Exception as e:
            self.handle_error("close", e)
            return -1

    def dagops(self) -> INodeDagops:
        if self.cached_dagops is None:
            self.cached_dagops = NodeDagops(self.env, self)
        return self.cached_dagops

    def get_next_name(self, base_name: str) -> str:
        return self.env.dagops.get_next_name(base_name)
