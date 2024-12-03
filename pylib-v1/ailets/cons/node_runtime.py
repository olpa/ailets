from dataclasses import dataclass
from typing import Dict, Optional, Sequence

from .node_dagops import NodeDagops
from .streams import Streams
from .atyping import (
    Dependency,
    IEnvironment,
    INodeDagops,
    INodeRegistry,
    INodeRuntime,
    IStream,
)


@dataclass
class OpenFd:
    stream: IStream
    pos: int


class NodeRuntime(INodeRuntime):
    def __init__(
        self,
        env: IEnvironment,
        nodereg: INodeRegistry,
        streams: Streams,
        node_name: str,
        deps: Sequence[Dependency],
    ):
        self._env = env
        self._nodereg = nodereg
        self._streams = streams
        self._node_name = node_name
        self._deps = deps
        self._open_fds: Dict[int, OpenFd] = {}

    def _get_streams(self, stream_name: Optional[str]) -> Sequence[IStream]:
        # Special stream "env"
        if stream_name == "env":
            return [self._env.get_env_stream()]
        # Normal explicit streams
        deps = [dep for dep in self._deps if dep.name == stream_name]
        # Implicit dynamic streams like media attachments
        if not deps and stream_name is not None:
            dep_names = set([dep.source for dep in self._deps])
            deps = [
                Dependency(name=stream_name, source=name, stream=stream_name)
                for name in dep_names
            ]
        return self._streams.collect_streams(deps)

    def get_name(self) -> str:
        return self._node_name

    def n_of_streams(self, stream_name: Optional[str]) -> int:
        if stream_name == "env":
            return 1
        return len(self._get_streams(stream_name))

    async def open_read(self, stream_name: Optional[str], index: int) -> int:
        streams = self._get_streams(stream_name)
        if index >= len(streams) or index < 0:
            raise ValueError(f"Stream index out of bounds: {index} for {stream_name}")
        fd = self._env.get_next_seqno()
        self._open_fds[fd] = OpenFd(stream=streams[index], pos=0)
        return fd

    async def read(self, fd: int, buffer: bytearray, count: int) -> int:
        fd_obj = self._open_fds[fd]
        read_bytes = await fd_obj.stream.read(fd_obj.pos, count)
        n_bytes = len(read_bytes)
        buffer[:n_bytes] = read_bytes
        fd_obj.pos += n_bytes
        return n_bytes

    async def open_write(self, stream_name: Optional[str]) -> int:
        stream = self._env.create_new_stream(self._node_name, stream_name)
        fd = self._env.get_next_seqno()
        self._open_fds[fd] = OpenFd(stream=stream, pos=0)
        return fd

    async def write(self, fd: int, buffer: bytes, count: int) -> int:
        fd_obj = self._open_fds[fd]
        return await fd_obj.stream.write(buffer)

    async def close(self, fd: int) -> None:
        fd_obj = self._open_fds.pop(fd)
        await fd_obj.stream.close()

    def dagops(self) -> INodeDagops:
        return NodeDagops(self._env, self._nodereg, self)

    async def read_dir(self, dir_name: str) -> Sequence[str]:
        dep_names = [dep.source for dep in self._deps]
        return await self._streams.read_dir(dir_name, [self._node_name, *dep_names])

    async def pass_through_name_name(
        self, in_stream_name: str, out_stream_name: str
    ) -> None:
        in_streams = self._get_streams(in_stream_name)
        for in_stream in in_streams:
            out_stream = self._env.create_new_stream(self._node_name, out_stream_name)
            await out_stream.write(await in_stream.read(pos=0, size=-1))
            await out_stream.close()

    async def pass_through_name_fd(self, in_stream_name: str, out_fd: int) -> None:
        in_streams = self._get_streams(in_stream_name)
        out_fd_obj = self._open_fds[out_fd]
        for in_stream in in_streams:
            await out_fd_obj.stream.write(await in_stream.read(pos=0, size=-1))

    def get_next_name(self, base_name: str) -> str:
        return self._env.get_next_name(base_name)
