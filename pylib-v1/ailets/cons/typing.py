from dataclasses import dataclass, field
from io import StringIO
from typing import (
    Any,
    Callable,
    Dict,
    Iterator,
    List,
    Optional,
    Protocol,
    Sequence,
    Set,
    Tuple,
)
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
    schema: Optional[dict] = None

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
    alias_of: Optional[str] = None


class INodeDagops(Protocol):
    def alias(self, alias: str, node_name: Optional[str]) -> None:
        raise NotImplementedError

    def expand_alias(self, alias: str) -> Sequence[str]:
        raise NotImplementedError

    def add_typed_value_node(
        self, value: str, value_type: str, explain: Optional[str] = None
    ) -> str:
        raise NotImplementedError

    def instantiate_with_deps(
        self,
        target: str,
        aliases: dict[str, str],
    ) -> str:
        raise NotImplementedError

    def defunc_downstream(self, upstream_node_name: str, fence: Set[str]) -> None:
        raise NotImplementedError

    def detach_from_alias(self, alias: str) -> None:
        raise NotImplementedError


class INodeRuntime(Protocol):
    def get_name(self) -> str:
        raise NotImplementedError

    def n_of_streams(self, node_name: Optional[str]) -> int:
        raise NotImplementedError

    def open_read(self, stream_name: Optional[str], index: int) -> StringIO:
        raise NotImplementedError

    def open_write(self, stream_name: Optional[str]) -> StringIO:
        raise NotImplementedError

    def close_write(self, stream_name: Optional[str]) -> None:
        raise NotImplementedError

    def get_plugin(self, regname: str) -> Sequence[str]:
        raise NotImplementedError

    def dagops(self) -> INodeDagops:
        raise NotImplementedError


@dataclass(frozen=True)
class NodeDescFunc:
    name: str
    inputs: Sequence[Dependency]
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

    def has_node(self, node_name: str) -> bool:
        raise NotImplementedError

    def is_node_ever_started(self, node_name: str) -> bool:
        raise NotImplementedError

    def alias(self, alias: str, node_name: Optional[str]) -> None:
        raise NotImplementedError

    def expand_alias(self, alias: str) -> Sequence[str]:
        raise NotImplementedError

    def add_typed_value_node(
        self, value: str, value_type: str, explain: Optional[str] = None
    ) -> Node:
        raise NotImplementedError

    def get_node(self, name: str) -> Node:
        raise NotImplementedError

    def clone_node(self, node_name: str) -> str:
        raise NotImplementedError

    def get_nodes(self) -> Sequence[Node]:
        raise NotImplementedError

    def iter_deps(self, node_name: str) -> Iterator[Dependency]:
        raise NotImplementedError

    def depend(self, target: str, deps: Sequence[Dependency]) -> None:
        raise NotImplementedError

    def get_node_by_base_name(self, base_name: str) -> Node:
        raise NotImplementedError

    def privates_for_dagops_friend(
        self,
    ) -> Tuple[Dict[str, Node], Dict[str, List[str]]]:
        raise NotImplementedError

    def get_next_name(self, full_name: str) -> str:
        raise NotImplementedError


class INodeRegistry(Protocol):
    def has_node(self, name: str) -> bool:
        raise NotImplementedError

    def get_node(self, name: str) -> NodeDescFunc:
        raise NotImplementedError

    def has_plugin(self, regname: str) -> bool:
        raise NotImplementedError

    def get_plugin(self, regname: str) -> Sequence[str]:
        raise NotImplementedError
