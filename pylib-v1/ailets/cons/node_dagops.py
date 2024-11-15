from typing import Optional, Sequence
from .typing import INodeDagops, IEnvironment, INodeRuntime, Dependency, BeginEnd


class NodeDagops(INodeDagops):
    def __init__(self, env: IEnvironment, node: INodeRuntime):
        self._env = env
        self._node = node

    def depend(self, target: str, source: Sequence[Dependency]) -> None:
        raise NotImplementedError

    def clone_path(self, begin: str, end: str) -> BeginEnd:
        raise NotImplementedError

    def add_typed_value_node(
        self, value: str, value_type: str, explain: Optional[str] = None
    ) -> str:
        raise NotImplementedError

    def add_node(
        self,
        name: str,
        deps: Optional[Sequence[Dependency]] = None,
        explain: Optional[str] = None,
    ) -> str:
        raise NotImplementedError

    def instantiate_tool(self, tool_name: str) -> BeginEnd:
        raise NotImplementedError
