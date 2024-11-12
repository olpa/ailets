from typing import Optional, Protocol
from .streams import Stream


class IEnvironment(Protocol):
    def create_new_stream(
        self, node_name: str, stream_name: Optional[str]
    ) -> Stream: ...

    def close_stream(self, stream: Stream) -> None: ...
