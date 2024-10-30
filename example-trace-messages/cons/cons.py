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
        """Get the cached result of a node. Does not build."""
        if name not in self.nodes:
            raise KeyError(f"Node {name} not found")

        node = self.nodes[name]
        if node.dirty or node.cache is None:
            raise RuntimeError(f"Node {name} is not built yet")
        return node.cache

    def build_node(self, name: str) -> Any:
        """Build a node and its dependencies if needed."""
        if name not in self.nodes:
            raise KeyError(f"Node {name} not found")

        node = self.nodes[name]

        # Build dependencies first
        dep_results = []
        for dep_name in node.deps:
            if (self.nodes[dep_name].dirty or
                    self.nodes[dep_name].cache is None):
                self.build_node(dep_name)
            dep_results.append(self.get_node(dep_name))

        # Execute the node's function with dependency results
        result = node.func(*dep_results)
        # Since Node is frozen, we need to create a new one with updated cache
        self.nodes[name] = Node(
            func=node.func, deps=node.deps, cache=result, dirty=False
        )
        return result


def mkenv() -> Environment:
    return Environment()
