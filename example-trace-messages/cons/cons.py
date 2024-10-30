from dataclasses import dataclass, field
from typing import Dict, Any, Callable, Set, Optional


@dataclass(frozen=True)
class Node:
    func: Callable[..., Any]
    deps: Set[str] = field(default_factory=set)
    cache: Any = field(default=None, compare=False)
    dirty: bool = field(default=True, compare=False)


class Environment:
    def __init__(self) -> None:
        self.nodes: Dict[str, Node] = {}

    def add_node(
        self,
        name: str,
        func: Callable[..., Any],
        deps: Optional[Set[str]] = None
    ) -> Node:
        """Add a build node with its dependencies."""
        deps = deps or set()
        node = Node(func=func, deps=deps)
        self.nodes[name] = node
        return node

    def get_node(self, name: str) -> Any:
        """Get the result of a node, building its dependencies first."""
        if name not in self.nodes:
            raise KeyError(f"Node {name} not found")

        node = self.nodes[name]
        if not node.dirty and node.cache is not None:
            return node.cache

        # Build dependencies first
        dep_results = [self.get_node(dep_name) for dep_name in node.deps]

        # Execute the node's function with dependency results
        result = node.func(*dep_results)
        # Since Node is frozen, we need to create a new one with updated cache
        self.nodes[name] = Node(
            func=node.func,
            deps=node.deps,
            cache=result,
            dirty=False
        )
        return result


def mkenv() -> Environment:
    return Environment()
