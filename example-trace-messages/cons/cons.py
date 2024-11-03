from dataclasses import dataclass, field
from typing import Dict, Any, Callable, Set, Optional, TextIO
import json


@dataclass(frozen=True)
class Node:
    name: str
    func: Callable[..., Any]
    deps: list[str] = field(default_factory=list)
    named_deps: Dict[str, list[str]] = field(default_factory=dict)
    cache: Any = field(default=None, compare=False)
    dirty: bool = field(default=True, compare=False)

    def to_json(self) -> Dict[str, Any]:
        """Convert node state to a JSON-serializable dict."""
        return {
            "name": self.name,
            "dirty": self.dirty,
            "deps": self.deps,
            "named_deps": self.named_deps,
            "cache": None if self.cache is None else str(self.cache),
            # Skip func as it's not serializable
        }


class Environment:
    def __init__(self) -> None:
        self.nodes: Dict[str, Node] = {}

    def add_node(
        self,
        name: str,
        func: Callable[..., Any],
        deps: Optional[list[str]] = None,
        named_deps: Optional[Dict[str, list[str]]] = None,
    ) -> Node:
        """Add a build node with its dependencies."""
        deps = deps or []
        named_deps = named_deps or {}
        node = Node(name=name, func=func, deps=deps, named_deps=named_deps)
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

        # Build default dependencies first
        dep_results = []
        for dep_name in node.deps:
            dep_node = self.get_node(dep_name)
            if dep_node.dirty or dep_node.cache is None:
                self.build_node(dep_name)
            dep_results.append(dep_node.cache)

        # Build named dependencies
        named_results = {}
        for param_name, dep_list in node.named_deps.items():
            param_results = []
            for dep_name in dep_list:
                dep_node = self.get_node(dep_name)
                if dep_node.dirty or dep_node.cache is None:
                    self.build_node(dep_name)
                param_results.append(dep_node.cache)
            named_results[param_name] = param_results

        # Execute the node's function with all dependencies
        try:
            result = node.func(*dep_results, **named_results)
        except Exception:
            print(f"Error building node '{name}'")
            print(f"Function: {node.func.__name__}")
            print(f"Default dependencies: {list(node.deps)}")
            print(f"Named dependencies: {dict(node.named_deps)}")
            raise

        # Since Node is frozen, we need to create a new one with updated cache
        self.nodes[name] = Node(
            name=node.name,
            func=node.func,
            deps=node.deps,
            named_deps=node.named_deps,
            cache=result,
            dirty=False,
        )
        return result

    def plan(self, target: str) -> list[str]:
        """Return nodes in build order for the target."""
        if target not in self.nodes:
            raise KeyError(f"Node {target} not found")

        visited: Set[str] = set()
        build_order: list[str] = []

        def visit(name: str) -> None:
            """DFS helper to build topological sort."""
            if name in visited:
                return

            if name in visiting:
                cycle = " -> ".join(visiting_list)
                raise RuntimeError(f"Cycle detected: {cycle}")

            visiting.add(name)
            visiting_list.append(name)

            # Visit all dependencies (both default and named)
            node = self.nodes[name]
            for dep in node.deps:
                visit(dep)
            for dep_list in node.named_deps.values():
                for dep in dep_list:
                    visit(dep)

            visiting.remove(name)
            visiting_list.pop()
            visited.add(name)
            build_order.append(name)

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

    def print_dependency_tree(
        self, node_name: str, indent: str = "", visited: Optional[Set[str]] = None
    ) -> None:
        """Print a tree showing node dependencies and build status.

        Args:
            node_name: Name of the node to print
            indent: Current indentation string (used for recursion)
            visited: Set of visited nodes to prevent cycles
        """
        if visited is None:
            visited = set()

        node = self.get_node(node_name)
        status = (
            "✓ built" if not node.dirty and node.cache is not None else "⋯ not built"
        )

        # Print current node
        print(f"{indent}├── {node.name} [{status}]")

        # Track visited nodes to prevent cycles
        if node_name in visited:
            print(f"{indent}│   └── (cycle detected)")
            return
        visited.add(node_name)

        # Print default dependencies
        if node.deps:
            print(f"{indent}│   ├── deps:")
            for dep in node.deps:
                self.print_dependency_tree(dep, f"{indent}│   │   ", visited)

        # Print named dependencies
        if node.named_deps:
            print(f"{indent}│   └── named deps:")
            for param_name, dep_list in node.named_deps.items():
                print(f"{indent}│       ├── {param_name}:")
                for dep in dep_list:
                    self.print_dependency_tree(dep, f"{indent}│       │   ", visited)

        visited.remove(node_name)


def mkenv() -> Environment:
    return Environment()
