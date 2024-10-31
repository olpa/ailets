from dataclasses import dataclass, field
from typing import Dict, Any, Callable, Set, Optional, TextIO
import json


@dataclass(frozen=True)
class Node:
    name: str
    func: Callable[..., Any]
    deps: Set[str] = field(default_factory=set)
    cache: Any = field(default=None, compare=False)
    dirty: bool = field(default=True, compare=False)

    def to_json(self) -> Dict[str, Any]:
        """Convert node state to a JSON-serializable dict."""
        return {
            "name": self.name,
            "dirty": self.dirty,
            "deps": list(self.deps),  # Convert set to list for JSON
            "cache": None if self.cache is None else str(self.cache),
            # Skip func as it's not serializable
        }


class Environment:
    def __init__(self) -> None:
        self.nodes: Dict[str, Node] = {}

    def add_node(
        self, name: str, func: Callable[..., Any], deps: Optional[Set[str]] = None
    ) -> Node:
        """Add a build node with its dependencies."""
        deps = deps or set()
        node = Node(name=name, func=func, deps=deps)
        self.nodes[name] = node
        return node

    def get_node(self, name: str) -> Node:
        """Get a node by name. Does not build."""
        if name not in self.nodes:
            raise KeyError(f"Node {name} not found")
        return self.nodes[name]

    def build_node(self, name: str) -> Any:
        """Build a node and its dependencies if needed."""
        node = self.get_node(name)

        # Build dependencies first
        dep_results = []
        for dep_name in node.deps:
            dep_node = self.get_node(dep_name)
            if dep_node.dirty or dep_node.cache is None:
                self.build_node(dep_name)
            dep_results.append(dep_node.cache)

        # Execute the node's function with dependency results
        try:
            result = node.func(*dep_results)
        except Exception:
            print(f"Error building node '{name}' with {len(dep_results)} dependencies")
            print(f"Function: {node.func.__name__}")
            print(f"Dependencies: {list(node.deps)}")
            raise

        # Since Node is frozen, we need to create a new one with updated cache
        self.nodes[name] = Node(
            name=node.name, func=node.func, deps=node.deps, cache=result, dirty=False
        )
        return result

    def plan(self, target: str) -> list[str]:
        """Return nodes in build order for the target."""
        if target not in self.nodes:
            raise KeyError(f"Node {target} not found")

        # Track visited nodes to detect cycles
        visited: Set[str] = set()
        # Store nodes in build order
        build_order: list[str] = []

        def visit(name: str) -> None:
            """DFS helper to build topological sort."""
            if name in visited:
                return

            # Check for cycles
            if name in visiting:
                cycle = " -> ".join(visiting_list)
                raise RuntimeError(f"Cycle detected: {cycle}")

            visiting.add(name)
            visiting_list.append(name)

            # Visit all dependencies first
            for dep in self.nodes[name].deps:
                visit(dep)

            visiting.remove(name)
            visiting_list.pop()
            visited.add(name)
            build_order.append(name)

        # Track nodes being visited in current DFS path
        visiting: Set[str] = set()
        visiting_list: list[str] = []

        visit(target)
        return build_order

    def serialize_node(self, name: str, stream: TextIO) -> None:
        """Serialize a node's state to a JSON stream."""
        if name not in self.nodes:
            raise KeyError(f"Node {name} not found")

        json.dump(self.nodes[name].to_json(), stream, indent=2)
        stream.write("\n")


def mkenv() -> Environment:
    return Environment()
