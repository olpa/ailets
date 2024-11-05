from dataclasses import dataclass, field
from typing import Dict, Any, Callable, Set, Optional, TextIO
import inspect
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
            "cache": None if self.cache is None else json.dumps(self.cache),
            # Skip func as it's not serializable
        }


class Environment:
    def __init__(self) -> None:
        self.nodes: Dict[str, Node] = {}
        self._node_counter: int = 0  # Single counter for all nodes

    def add_node(
        self,
        name: str,
        func: Callable[..., Any],
        deps: Optional[list[str]] = None,
        named_deps: Optional[Dict[str, list[str]]] = None,
    ) -> Node:
        """Add a build node with its dependencies.

        The node name will automatically get a suffix '.N' where N is an incrementing
        number shared across all nodes.
        """
        self._node_counter += 1
        full_name = f"{name}.{self._node_counter}"

        # Create and add node
        deps = deps or []
        named_deps = named_deps or {}
        node = Node(name=full_name, func=func, deps=deps, named_deps=named_deps)
        self.nodes[full_name] = node
        return node

    def get_node(self, name: str) -> Node:
        """Get a node by name. Does not build."""
        if name not in self.nodes:
            raise KeyError(f"Node {name} not found")
        return self.nodes[name]

    def get_node_by_base_name(self, base_name: str) -> Node:
        """Get a node by its base name (without the numeric suffix).

        Args:
            base_name: Name of node without the numeric suffix

        Returns:
            The node with the given base name

        Raises:
            KeyError: If no node with the given base name exists
        """
        for name, node in self.nodes.items():
            if name.rsplit(".", 1)[0] == base_name:
                return node
        raise KeyError(f"No node found with base name {base_name}")

    def build_node(self, name: str) -> Any:
        """Build a node and its dependencies if needed."""
        node = self.get_node(name)

        # If node is already built and clean, return cached result
        if not node.dirty and node.cache is not None:
            return node.cache

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
            # Get function signature to check for env/node params
            sig = inspect.signature(node.func)
            kwargs: Dict[str, Any] = named_results.copy()

            # Add env and node parameters if the function accepts them
            if "env" in sig.parameters:
                kwargs["env"] = self
            if "node" in sig.parameters:
                kwargs["node"] = node

            result = node.func(*dep_results, **kwargs)
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

        # Print all dependencies with proper indentation
        next_indent = f"{indent}│   "

        # Print regular dependencies
        for dep in node.deps:
            self.print_dependency_tree(dep, next_indent, visited)

        # Print named dependencies with their parameter names
        for param_name, dep_list in node.named_deps.items():
            print(f"{next_indent}├── (param: {param_name})")
            param_indent = f"{next_indent}│   "
            for dep in dep_list:
                self.print_dependency_tree(dep, param_indent, visited.copy())

        visited.remove(node_name)

    def load_node_state(
        self, node_data: Dict[str, Any], func_map: Dict[str, Callable[..., Any]]
    ) -> None:
        """Load a node's state from JSON data.

        Args:
            node_data: Node state from JSON
            func_map: Mapping from node names to their functions
        """
        name = node_data["name"]

        # Try to get function from map, if not found and name has a number suffix,
        # try without the suffix
        if name not in func_map and "." in name:
            base_name = name.rsplit(".", 1)[0]  # Get name without the suffix
            func = func_map.get(base_name)
            if func is None:
                raise KeyError(f"No function provided for node: {name} or {base_name}")
        else:
            func = func_map.get(name)
            if func is None:
                raise KeyError(f"No function provided for node: {name}")

        # Create new node with loaded state
        cache_str = node_data["cache"]
        self.nodes[name] = Node(
            name=name,
            func=func,
            deps=node_data["deps"],
            named_deps=node_data["named_deps"],
            cache=None if cache_str is None else json.loads(cache_str),
            dirty=node_data["dirty"],
        )

    def clone_path(self, start: str, end: str) -> list[Node]:
        """Clone a path of nodes from start to end.

        Args:
            start: Name of starting node
            end: Name of ending node

        Returns:
            List of nodes in the cloned path. First element is the cloned start node,
            last element is the cloned end node. Order of other nodes is not guaranteed.
        """
        # Track which nodes have been cloned and their clones
        original_to_clone: Dict[str, str] = {}
        to_clone: Set[str] = {start}
        cloned: Set[str] = set()

        # Track which dependencies should be named in the cloned graph
        named_deps_mapping: Dict[str, Set[tuple[str, str]]] = (
            {}
        )  # target -> set of (dep, param_name)

        # First, clone the start node's dependencies
        start_node = self.get_node(start)
        for dep in start_node.deps:
            to_clone.add(dep)
        for param_name, dep_list in start_node.named_deps.items():
            for dep in dep_list:
                to_clone.add(dep)
                # Record that this dependency should be named in the clone
                if start not in named_deps_mapping:
                    named_deps_mapping[start] = set()
                named_deps_mapping[start].add((dep, param_name))

        while to_clone:
            # Get next node to clone
            current_name = to_clone.pop()
            if current_name in cloned:
                continue

            current = self.get_node(current_name)

            # Create clone (initially without dependencies)
            clone = self.add_node(
                current_name, current.func
            )  # Will get next number automatically
            original_to_clone[current_name] = clone.name
            cloned.add(current_name)

            # Stop expanding at end node
            if current_name == end:
                continue

            # Add all next nodes to the to_clone set
            next_nodes = self.get_next_nodes(current)
            for next_node in next_nodes:
                to_clone.add(next_node.name)

        # Recreate dependencies between cloned nodes by creating new nodes
        for original_name, clone_name in original_to_clone.items():
            original = self.get_node(original_name)
            clone = self.get_node(clone_name)
            new_named_deps: Dict[str, list[str]] = {}

            if original_name == start:
                # For start node, keep original dependencies
                new_deps = list(original.deps)
                new_named_deps = dict(original.named_deps)
            else:
                # For other nodes, use cloned dependencies
                new_deps = [
                    original_to_clone[dep]
                    for dep in original.deps
                    if dep in original_to_clone
                ]

                # First, copy existing named dependencies that were cloned
                for param_name, deps in original.named_deps.items():
                    cloned_deps = [
                        original_to_clone[dep]
                        for dep in deps
                        if dep in original_to_clone
                    ]
                    if cloned_deps:
                        new_named_deps[param_name] = cloned_deps

                # Then add any dependencies that should be named in this clone
                if original_name in named_deps_mapping:
                    for dep, param_name in named_deps_mapping[original_name]:
                        if (
                            dep in original_to_clone
                        ):  # Only if the dependency was cloned
                            if param_name not in new_named_deps:
                                new_named_deps[param_name] = []
                            new_named_deps[param_name].append(original_to_clone[dep])

            # Create new node with dependencies
            self.nodes[clone_name] = Node(
                name=clone_name,
                func=clone.func,
                deps=new_deps,
                named_deps=new_named_deps,
                cache=clone.cache,
                dirty=clone.dirty,
            )

        # Create return list with start and end nodes in correct positions
        result = []
        # Add start node first
        result.append(self.nodes[original_to_clone[start]])
        # Add middle nodes in any order
        for original_name, clone_name in original_to_clone.items():
            if original_name not in (start, end):
                result.append(self.nodes[clone_name])
        # Add end node last
        result.append(self.nodes[original_to_clone[end]])

        return result

    def get_next_nodes(self, node: Node) -> list[Node]:
        """Return list of nodes that depend on the given node."""
        next_nodes = []
        for other_node in self.nodes.values():
            # Check regular dependencies
            if node.name in other_node.deps:
                next_nodes.append(other_node)
            # Check named dependencies
            for dep_list in other_node.named_deps.values():
                if node.name in dep_list:
                    next_nodes.append(other_node)
                    break
        return next_nodes


def mkenv() -> Environment:
    return Environment()
