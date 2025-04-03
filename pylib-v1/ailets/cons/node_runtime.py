from dataclasses import dataclass
from typing import Dict, Optional, Sequence

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

    def _get_streams(self, stream_name: str) -> Sequence[IPipe]:
        # Special streams "env" and "log"
        if stream_name == "env":
            return [Piper.make_env_stream(self.env.for_env_stream)]
        if stream_name == "log":
            return [Piper.make_log_stream()]
        # Normal explicit streams
        deps = [dep for dep in self.deps if dep.name == stream_name]
        # Implicit dynamic streams like media attachments
        if not deps and stream_name is not None:
            dep_names = set([dep.source for dep in self.deps])
            deps = [
                Dependency(name=stream_name, source=name, stream=stream_name)
                for name in dep_names
            ]
        # Collect
        pipes = []
        for dep in deps:
            try:
                pipe = self.piper.get_existing_pipe(dep.source, dep.stream)
            except KeyError:
                continue
            pipes.append(pipe)
        return pipes

    def get_name(self) -> str:
        return self.node_name

    def n_of_streams(self, stream_name: str) -> int:
        if stream_name == "env":
            return 1
        return len(self._get_streams(stream_name))

    async def open_read(self, stream_name: str, index: int) -> int:
        streams = self._get_streams(stream_name)
        if index >= len(streams) or index < 0:
            raise ValueError(f"Stream index out of bounds: {index} for {stream_name}")
        fd = self.env.seqno.next_seqno()
        reader = streams[index].get_reader(fd)
        self.open_fds[fd] = OpenFd(
            debug_hint=f"{self.node_name}.{stream_name}[{index}]",
            reader=reader,
            writer=None,
        )
        return fd

    async def read(self, fd: int, buffer: bytearray, count: int) -> int:
        assert fd in self.open_fds, f"File descriptor {fd} is not open"
        fd_obj = self.open_fds[fd]
        assert (
            fd_obj.reader is not None
        ), f"Stream {fd_obj.debug_hint} is not open for reading"
        read_bytes = await fd_obj.reader.read(count)
        n_bytes = len(read_bytes)
        buffer[:n_bytes] = read_bytes
        return n_bytes

    async def open_write(self, stream_name: str) -> int:
        stream = self.piper.create_pipe(self.node_name, stream_name)
        fd = self.env.seqno.next_seqno()
        writer = stream.get_writer()
        self.open_fds[fd] = OpenFd(
            debug_hint=f"{self.node_name}.{stream_name}",
            reader=None,
            writer=writer,
        )
        return fd

    async def write(self, fd: int, buffer: bytes, count: int) -> int:
        assert fd in self.open_fds, f"File descriptor {fd} is not open"
        fd_obj = self.open_fds[fd]
        assert (
            fd_obj.writer is not None
        ), f"Stream {fd_obj.debug_hint} is not open for writing"
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
        return self.env.kv.read_dir(dir_name)

    async def pass_through_name_name(
        self, in_stream_name: str, out_stream_name: str
    ) -> None:
        in_streams = self._get_streams(in_stream_name)
        for in_stream in in_streams:
            reader = in_stream.get_reader(self.env.seqno.next_seqno())
            out_stream = self.piper.create_pipe(self.node_name, out_stream_name)
            writer = out_stream.get_writer()
            await writer.write(await reader.read(size=-1))
            writer.close()

    async def pass_through_name_fd(self, in_stream_name: str, out_fd: int) -> None:
        in_streams = self._get_streams(in_stream_name)
        out_fd_obj = self.open_fds[out_fd]
        assert (
            out_fd_obj.writer is not None
        ), f"File descriptor {out_fd} is not open for writing"
        for in_stream in in_streams:
            reader = in_stream.get_reader(self.env.seqno.next_seqno())
            await out_fd_obj.writer.write(await reader.read(size=-1))

    def get_next_name(self, base_name: str) -> str:
        return self.env.dagops.get_next_name(base_name)
