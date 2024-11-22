from dataclasses import dataclass, field
from io import StringIO
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

    def dagops(self) -> INodeDagops:
        raise NotImplementedError

    def log(self, level: Literal["info", "warn", "error"], *message: Any) -> None:
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

    def add_typed_value_node(
        self, value: str, value_type: str, explain: Optional[str] = None
    ) -> Node:
        raise NotImplementedError

    def iter_deps(self, node_name: str) -> Iterator[Dependency]:
        raise NotImplementedError

    def depend(self, target: str, deps: Sequence[Dependency]) -> None:
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


class ChatMessageContentImageUrl(TypedDict):
    image_url: dict[Literal["url", "detail"], str]
    type: Literal["image_url"]


class ChatMessageContentInputAudio(TypedDict):
    input_audio: dict[Literal["data", "format"], str]
    type: Literal["input_audio"]


class ChatMessageContentRefusal(TypedDict):
    refusal: str
    type: Literal["refusal"]


class ChatAssistantToolCall(TypedDict):
    id: str
    function: dict[Literal["name", "arguments"], str]
    type: Literal["function"]


ChatMessageStructuredContentItem = Union[
    ChatMessageContentText,
    ChatMessageContentImageUrl,
    ChatMessageContentInputAudio,
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
    content: Optional[ChatMessageContent]
    refusal: NotRequired[str]
    # `tool_calls` should be handled inside a model pipeline (gpt4o, etc.)
    # The generic chat-to-something converter expects only the final result
    # tool_calls: Optional[Sequence[ChatAssistantToolCall]]
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
