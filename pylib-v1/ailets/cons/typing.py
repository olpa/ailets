from dataclasses import dataclass, field
from io import BytesIO
from typing import (
    Any,
    Callable,
    Dict,
    Iterator,
    List,
    Literal,
    NotRequired,
    Optional,
    Protocol,
    Sequence,
    Tuple,
    TypedDict,
    Union,
)


class IStream(Protocol):
    def get_content(self) -> BytesIO:
        raise NotImplementedError

    def close(self) -> None:
        raise NotImplementedError


#
#
#


@dataclass(frozen=True)
class Dependency:
    """A dependency of a node on another node's stream.

    Attributes:
        name: Optional name to reference this dependency in the node's inputs
        source: Name of the node this dependency comes from
        stream: Optional name of the specific stream from the source node
        schema: Optional schema for the stream
    """

    source: str
    name: Optional[str] = None
    stream: Optional[str] = None
    schema: Optional[dict] = None

    def to_json(self) -> list:
        """Convert to JSON-serializable format.

        Returns:
            List of [dep_name, node_name, stream_name, schema]
        """
        return [self.name, self.source, self.stream, self.schema]

    @classmethod
    def from_json(cls, data: list) -> "Dependency":
        """Create dependency from JSON data.

        Args:
            data: List of [dep_name, node_name, stream_name, schema]
        """
        return cls(name=data[0], source=data[1], stream=data[2], schema=data[3])


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

    def add_value_node(self, value: bytes, explain: Optional[str] = None) -> str:
        raise NotImplementedError

    def instantiate_with_deps(
        self,
        target: str,
        aliases: dict[str, str],
    ) -> str:
        raise NotImplementedError

    def detach_from_alias(self, alias: str) -> None:
        raise NotImplementedError


class INodeRuntime(Protocol):
    def get_name(self) -> str:
        raise NotImplementedError

    def n_of_streams(self, stream_name: Optional[str]) -> int:
        raise NotImplementedError

    def open_read(self, stream_name: Optional[str], index: int) -> int:
        raise NotImplementedError

    def open_write(self, stream_name: Optional[str]) -> int:
        raise NotImplementedError

    def read(self, fd: int, buffer: bytearray, count: int) -> int:
        raise NotImplementedError

    def write(self, fd: int, buffer: bytes, count: int) -> int:
        raise NotImplementedError

    def close(self, fd: int) -> None:
        raise NotImplementedError

    def dagops(self) -> INodeDagops:
        raise NotImplementedError

    def get_next_name(self, base_name: str) -> str:
        raise NotImplementedError

    def read_dir(self, dir_name: str) -> Sequence[str]:
        raise NotImplementedError

    def pass_through(self, in_stream_name: str, out_stream_name: str) -> None:
        raise NotImplementedError


@dataclass(frozen=True)
class NodeDescFunc:
    name: str
    inputs: Sequence[Dependency]
    func: Callable[[INodeRuntime], None]


class IEnvironment(Protocol):
    def create_new_stream(self, node_name: str, stream_name: Optional[str]) -> IStream:
        raise NotImplementedError

    def has_node(self, node_name: str) -> bool:
        raise NotImplementedError

    def add_node(
        self,
        name: str,
        func: Callable[..., Any],
        deps: Optional[Sequence[Dependency]] = None,
        explain: Optional[str] = None,
    ) -> Node:
        raise NotImplementedError

    def is_node_ever_started(self, node_name: str) -> bool:
        raise NotImplementedError

    def alias(self, alias: str, node_name: Optional[str]) -> None:
        raise NotImplementedError

    def add_value_node(self, value: bytes, explain: Optional[str] = None) -> Node:
        raise NotImplementedError

    def iter_deps(self, node_name: str) -> Iterator[Dependency]:
        raise NotImplementedError

    def depend(self, target: str, deps: Sequence[Dependency]) -> None:
        raise NotImplementedError

    def privates_for_dagops_friend(
        self,
    ) -> Tuple[Dict[str, Node], Dict[str, List[str]]]:
        raise NotImplementedError

    def get_next_seqno(self) -> int:
        raise NotImplementedError

    def get_next_name(self, full_name: str) -> str:
        raise NotImplementedError

    def update_for_env_stream(self, params: Dict[str, Any]) -> None:
        raise NotImplementedError

    def get_env_stream(self) -> IStream:
        raise NotImplementedError

    def read_dir(self, node_name: str, dir_name: str) -> Sequence[str]:
        raise NotImplementedError

    def pass_through(
        self,
        node_name: str,
        in_stream_name: str,
        out_stream_name: str,
    ) -> None:
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


#
#
#
class ToolSpecification(TypedDict):
    name: str
    description: str
    parameters: dict[str, Any]  # JSON schema


ChatMessageContentPlainText = str


class ChatMessageContentText(TypedDict):
    type: Literal["text"]
    text: str


class ChatMessageContentImage(TypedDict):
    type: Literal["image"]
    content_type: str
    url: NotRequired[str]
    stream: NotRequired[str]


class ChatMessageContentRefusal(TypedDict):
    refusal: str
    type: Literal["refusal"]


class ChatAssistantToolCall(TypedDict):
    id: str
    function: dict[Literal["name", "arguments"], str]
    type: Literal["function"]


ChatMessageStructuredContentItem = Union[
    ChatMessageContentText,
    ChatMessageContentImage,
    ChatMessageContentRefusal,
]

ChatMessageStructuredContent = Sequence[ChatMessageStructuredContentItem]

ChatMessageContent = Union[
    ChatMessageContentPlainText,
    ChatMessageStructuredContent,
]


class ChatMessageSystem(TypedDict):
    content: ChatMessageContentPlainText
    role: Literal["system"]


class ChatMessageUser(TypedDict):
    content: ChatMessageContent
    role: Literal["user"]


class ChatMessageAssistant(TypedDict):
    content: NotRequired[Union[ChatMessageContent, None]]  # If None, then "tool_calls"
    refusal: NotRequired[str]
    tool_calls: NotRequired[Sequence[ChatAssistantToolCall]]
    role: Literal["assistant"]


class ChatMessageToolCall(TypedDict):
    tool_call_id: str
    content: ChatMessageContent
    role: Literal["tool"]


ChatMessage = Union[
    ChatMessageSystem,
    ChatMessageUser,
    ChatMessageAssistant,
    ChatMessageToolCall,
]
