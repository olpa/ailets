from typing import Dict, Mapping, Optional, Sequence
from io import StringIO
from .streams import Stream
from .typing import IEnvironment, INodeRuntime


from typing import Protocol

class NodeRuntime(INodeRuntime):
    def __init__(
        self,
        env: IEnvironment,
        streams: Mapping[Optional[str], Sequence[Stream]],
        node_name: str,
    ):
        self._env = env
        self._streams = streams
        self._node_name = node_name
        self._write_streams: Dict[Optional[str], Stream] = {}

    def _get_streams(self, node_name: Optional[str]) -> Sequence[Stream]:
        return self._streams.get(node_name, [])

    def n_of_streams(self, node_name: Optional[str]) -> int:
        return len(self._get_streams(node_name))

    def open_read(self, stream_name: Optional[str], index: int) -> StringIO:
        streams = self._get_streams(stream_name)
        if index >= len(streams) or index < 0:
            raise ValueError(f"Stream index out of bounds: {index} for {stream_name}")
        sio = streams[index].content
        sio.seek(0)
        return sio

    def open_write(self, stream_name: Optional[str]) -> StringIO:
        stream = self._env.create_new_stream(self._node_name, stream_name)
        self._write_streams[stream_name] = stream
        return stream.content

    def close_write(self, stream_name: Optional[str]) -> None:
        stream = self._write_streams.pop(stream_name)
        self._env.close_stream(stream)
