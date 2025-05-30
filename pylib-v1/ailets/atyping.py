from dataclasses import dataclass, field
from enum import IntEnum
from typing import (
    Any,
    Awaitable,
    Callable,
    Dict,
    Iterator,
    List,
    Literal,
    Optional,
    Protocol,
    Sequence,
    Tuple,
    TypedDict,
    Union,
)
from typing_extensions import NotRequired
from ailets.cons.notification_queue import INotificationQueue
from ailets.cons.seqno import Seqno


class IAsyncReader(Protocol):
    closed: bool

    async def read(self, size: int) -> bytes:
        raise NotImplementedError

    def close(self) -> None:
        raise NotImplementedError

    def set_error(self, errno: int) -> None:
        raise NotImplementedError


class IAsyncWriter(Protocol):
    closed: bool

    async def write(self, data: bytes) -> int:
        raise NotImplementedError

    def close(self) -> None:
        raise NotImplementedError

    def tell(self) -> int:
        raise NotImplementedError

    def get_error(self) -> int:
        raise NotImplementedError

    def set_error(self, errno: int) -> None:
        raise NotImplementedError


class IPipe(Protocol):
    def get_writer(self) -> IAsyncWriter:
        raise NotImplementedError

    def get_reader(self, handle: int) -> IAsyncReader:
        raise NotImplementedError


class IKVBuffer(Protocol):
    def borrow_mut_buffer(self) -> bytearray:
        raise NotImplementedError


class IKVBuffers(Protocol):
    def open(self, path: str, mode: Literal["read", "write", "append"]) -> IKVBuffer:
        raise NotImplementedError

    def flush(self, path: str) -> None:
        raise NotImplementedError

    def listdir(self, dir_name: str) -> Sequence[str]:
        raise NotImplementedError

    def destroy(self) -> None:
        raise NotImplementedError


class IPiper(Protocol):
    def destroy(self) -> None:
        """Release fsops handle"""
        raise NotImplementedError

    def create_pipe(
        self,
        node_name: str,
        slot_name: str,
        open_mode: Literal["read", "write", "append"] = "write",
    ) -> IPipe:
        raise NotImplementedError

    def get_existing_pipe(self, node_name: str, slot_name: str) -> IPipe:
        """If not found, raise KeyError
        If called before the pipe is created: unusable for reading"""
        raise NotImplementedError

    def get_fsops_handle(self) -> int:
        raise NotImplementedError

    def flush_pipes(self) -> None:
        raise NotImplementedError


class StdHandles(IntEnum):
    stdin = 0
    stdout = 1
    log = 2
    env = 3
    metrics = 4
    trace = 5


#
#
#


@dataclass(frozen=True)
class Dependency:
    """A dependency of a node on another node's.

    Attributes:
        name: Optional name to reference this dependency in the node's inputs
        source: Name of the node this dependency comes from
        slot: Optional name of the specific slot from the source node
        schema: Optional schema for the slot
    """

    source: str
    name: str = ""
    slot: str = ""
    schema: Optional[dict[str, Any]] = None


@dataclass(frozen=True)
class Node:
    name: str
    func: Callable[..., Awaitable[Any]]
    deps: List[Dependency] = field(default_factory=list)  # [(node_name, dep_name)]
    explain: Optional[str] = field(default=None)  # New field for explanation


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

    def v2_alias(self, alias: str, node_handle: int) -> int:
        raise NotImplementedError

    def v2_add_value_node(self, value: bytes, explain: Optional[str] = None) -> int:
        raise NotImplementedError

    def v2_instantiate_with_deps(self, target: str, aliases: dict[str, int]) -> int:
        raise NotImplementedError


class INodeRuntime(Protocol):
    def get_name(self) -> str:
        raise NotImplementedError

    def get_errno(self) -> int:
        raise NotImplementedError

    def set_errno(self, errno: int) -> None:
        raise NotImplementedError

    async def open_read(self, slot_name: str) -> int:
        raise NotImplementedError

    async def open_write(self, slot_name: str) -> int:
        raise NotImplementedError

    async def read(self, fd: int, buffer: bytearray, count: int) -> int:
        raise NotImplementedError

    async def write(self, fd: int, buffer: bytes, count: int) -> int:
        raise NotImplementedError

    async def close(self, fd: int) -> int:
        raise NotImplementedError

    def dagops(self) -> INodeDagops:
        raise NotImplementedError

    def get_next_name(self, base_name: str) -> str:
        raise NotImplementedError


@dataclass(frozen=True)
class NodeDescFunc:
    name: str
    inputs: Sequence[Dependency]
    func: Callable[[INodeRuntime], Awaitable[None]]


class IDagops(Protocol):
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

    def get_node(self, name: str) -> Node:
        raise NotImplementedError

    def get_node_names(self) -> Sequence[str]:
        raise NotImplementedError

    def alias(self, alias: str, node_name: Optional[str]) -> None:
        raise NotImplementedError

    def add_value_node(
        self,
        value: bytes,
        piper: IPiper,
        processes: "IProcesses",
        explain: Optional[str] = None,
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

    def hash_of_nodenames(self) -> int:
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


class IWasmRegistry(Protocol):
    def get_module(self, name: str) -> bytes:
        raise NotImplementedError


class IProcesses(Protocol):
    def next_node_iter(
        self,
        target_node_name: str,
        flag_one_step: bool,
        stop_before: Optional[str],
        stop_after: Optional[str],
    ) -> Iterator[str | None]:
        raise NotImplementedError

    async def build_node_alone(self, name: str) -> None:
        raise NotImplementedError

    def is_node_finished(self, name: str) -> bool:
        raise NotImplementedError

    def is_node_active(self, name: str) -> bool:
        raise NotImplementedError

    def add_finished_node(self, name: str) -> None:
        raise NotImplementedError

    def set_completion_code(self, name: str, ccode: int) -> None:
        raise NotImplementedError

    def get_optional_completion_code(self, name: str) -> Optional[int]:
        raise NotImplementedError


class IEnvironment(Protocol):
    for_env_pipe: Dict[str, Any]
    seqno: Seqno
    dagops: IDagops
    piper: IPiper
    kv: IKVBuffers
    nodereg: INodeRegistry
    processes: IProcesses
    notification_queue: INotificationQueue

    def get_errno(self) -> int:
        raise NotImplementedError

    def set_errno(self, errno: int) -> None:
        raise NotImplementedError


#
#
#


class ToolSpecification(TypedDict):
    name: str
    description: str
    parameters: dict[str, Any]  # JSON schema


#
#
#


class ContentItemTextAttrs(TypedDict):
    type: Literal["text"]
    _role: NotRequired[Literal["system", "user", "assistant"]]  # To be removed after refactoring, ticket #162

class ContentItemTextContent(TypedDict):
    text: str

ContentItemText = Tuple[ContentItemTextAttrs, ContentItemTextContent]


class ContentItemImageAttrs(TypedDict):
    type: Literal["image"]
    content_type: str

class ContentItemImageContent(TypedDict):
    image_url: NotRequired[str]
    image_key: NotRequired[str]

ContentItemImage = Tuple[ContentItemImageAttrs, ContentItemImageContent]


class ContentItemRefusalAttrs(TypedDict):
    type: Literal["refusal"]

class ContentItemRefusalContent(TypedDict):
    refusal: str

ContentItemRefusal = Tuple[ContentItemRefusalAttrs, ContentItemRefusalContent]


class ContentItemFunctionAttrs(TypedDict):
    type: Literal["function"]
    id: str
    name: str

class ContentItemFunctionContent(TypedDict):
    arguments: str

ContentItemFunction = Tuple[ContentItemFunctionAttrs, ContentItemFunctionContent]


class ContentItemCtlAttrs(TypedDict):
    type: Literal["ctl"]

class ContentItemCtlContent(TypedDict):
    role: str

ContentItemCtl = Tuple[ContentItemCtlAttrs, ContentItemCtlContent]


ContentItem = Union[
    ContentItemText,
    ContentItemImage,
    ContentItemRefusal,
    ContentItemFunction,
    ContentItemCtl,
]
