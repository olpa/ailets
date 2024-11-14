from dataclasses import dataclass
from io import StringIO
from typing import Callable, Optional, Protocol, Sequence
from .streams import Stream


@dataclass(frozen=True)
class Dependency:
    """A dependency of a node on another node's stream.

    Attributes:
        name: Optional name to reference this dependency in the node's inputs
        source: Name of the node this dependency comes from
        stream: Optional name of the specific stream from the source node
    """

    source: str
    name: Optional[str] = None
    stream: Optional[str] = None

    def to_json(self) -> list:
        """Convert to JSON-serializable format.

        Returns:
            List of [dep_name, node_name, stream_name]
        """
        return [self.name, self.source, self.stream]

    @classmethod
    def from_json(cls, data: list) -> "Dependency":
        """Create dependency from JSON data.

        Args:
            data: List of [dep_name, node_name, stream_name]
        """
        return cls(name=data[0], source=data[1], stream=data[2])


@dataclass(frozen=True)
class NodeDesc:
    name: str
    inputs: Sequence[Dependency]


class INodeRuntime(Protocol):
    def n_of_streams(self, node_name: Optional[str]) -> int:
        raise NotImplementedError

    def open_read(self, stream_name: Optional[str], index: int) -> StringIO:
        raise NotImplementedError

    def open_write(self, stream_name: Optional[str]) -> StringIO:
        raise NotImplementedError

    def close_write(self, stream_name: Optional[str]) -> None:
        raise NotImplementedError


@dataclass(frozen=True)
class NodeDescFunc(NodeDesc):
    func: Callable[[INodeRuntime], None]


class IEnvironment(Protocol):
    def create_new_stream(self, node_name: str, stream_name: Optional[str]) -> Stream:
        raise NotImplementedError

    def close_stream(self, stream: Stream) -> None:
        raise NotImplementedError
