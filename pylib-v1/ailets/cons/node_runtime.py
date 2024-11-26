from typing import Dict, Mapping, Optional, Sequence

from .node_dagops import NodeDagops
from .streams import Stream
from .typing import IEnvironment, INodeDagops, INodeRegistry, INodeRuntime


class NodeRuntime(INodeRuntime):
    def __init__(
        self,
        env: IEnvironment,
        nodereg: INodeRegistry,
        streams: Mapping[Optional[str], Sequence[Stream]],
        node_name: str,
    ):
        self._env = env
        self._nodereg = nodereg
        self._streams = streams
        self._node_name = node_name
        self._open_fds: Dict[int, Stream] = {}

    def _get_streams(self, stream_name: Optional[str]) -> Sequence[Stream]:
        if stream_name == "env":
            return [self._env.get_env_stream()]
        return self._streams.get(stream_name, [])

    def get_name(self) -> str:
        return self._node_name

    def n_of_streams(self, stream_name: Optional[str]) -> int:
        if stream_name == "env":
            return 1
        return len(self._get_streams(stream_name))

    def open_read(self, stream_name: Optional[str], index: int) -> int:
        streams = self._get_streams(stream_name)
        if index >= len(streams) or index < 0:
            raise ValueError(f"Stream index out of bounds: {index} for {stream_name}")
        bio = streams[index].content
        bio.seek(0)
        fd = self._env.get_next_seqno()
        self._open_fds[fd] = streams[index]
        return fd

    def read(self, fd: int, buffer: bytearray, count: int) -> int:
        stream = self._open_fds[fd]
        return stream.content.readinto(buffer)

    def open_write(self, stream_name: Optional[str]) -> int:
        stream = self._env.create_new_stream(self._node_name, stream_name)
        fd = self._env.get_next_seqno()
        self._open_fds[fd] = stream
        return fd

    def write(self, fd: int, buffer: bytes, count: int) -> int:
        stream = self._open_fds[fd]
        return stream.content.write(buffer)

    def close(self, fd: int) -> None:
        stream = self._open_fds.pop(fd)
        stream.is_finished = True

    def dagops(self) -> INodeDagops:
        return NodeDagops(self._env, self._nodereg, self)

    def read_dir(self, dir_name: str) -> Sequence[str]:
        return self._env.read_dir(self._node_name, dir_name)

    def pass_through(self, in_stream_name: str, out_stream_name: str) -> None:
        self._env.pass_through(self._node_name, in_stream_name, out_stream_name)

    def get_next_name(self, base_name: str) -> str:
        return self._env.get_next_name(base_name)
