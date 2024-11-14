from dataclasses import dataclass, field
from io import StringIO
from typing import Any, Callable, Dict, List, Optional, Protocol, Sequence
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
class Node:
    name: str
    func: Callable[..., Any]
    deps: List[Dependency] = field(default_factory=list)  # [(node_name, dep_name)]
    explain: Optional[str] = field(default=None)  # New field for explanation

    def to_json(self) -> Dict[str, Any]:
        """Convert node state to a JSON-serializable dict."""
        return {
            "name": self.name,
            "deps": [dep.to_json() for dep in self.deps],
            "explain": self.explain,  # Add explain field to JSON
            # Skip func as it's not serializable
        }


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

    def get_tool(self, name: str) -> tuple[Callable, Callable]:
        raise NotImplementedError

    def add_node(
        self,
        name: str,
        func: Callable[..., Any],
        deps: Optional[Sequence[Dependency]] = None,
        explain: Optional[str] = None,
    ) -> Node:
        raise NotImplementedError

    def alias(self, alias: str, node_name: str) -> None:
        raise NotImplementedError

    def add_typed_value_node(
        self, value: str, value_type: str, explain: Optional[str] = None
    ) -> Node:
        raise NotImplementedError
