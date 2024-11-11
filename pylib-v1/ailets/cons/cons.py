from dataclasses import dataclass, field
from typing import Dict, Any, Callable, Set, Optional, TextIO, Union, Sequence, List
import inspect
import json
from io import StringIO


@dataclass(frozen=True)
class Node:
    name: str
    func: Callable[..., Any]
    deps: List[tuple[str, Optional[str]]] = field(
        default_factory=list
    )  # [(node_name, dep_name)]
    cache: Any = field(default=None, compare=False)
    dirty: bool = field(default=True, compare=False)
    explain: Optional[str] = field(default=None)  # New field for explanation

    def to_json(self) -> Dict[str, Any]:
        """Convert node state to a JSON-serializable dict."""
        return {
            "name": self.name,
            "dirty": self.dirty,
            "deps": self.deps,
            "cache": None if self.cache is None else json.dumps(self.cache),
            "explain": self.explain,  # Add explain field to JSON
            # Skip func as it's not serializable
        }


@dataclass
class Stream:
    """A stream of data associated with a node.

    Attributes:
        node_name: Name of the node this stream belongs to
        stream_name: Name of the stream
        is_finished: Whether the stream is complete
        content: The StringIO buffer containing the stream data
    """

    node_name: str
    stream_name: str
    is_finished: bool
    content: StringIO

    def to_json(self) -> dict:
        """Convert stream to JSON-serializable dict."""
        return {
            "node": self.node_name,
            "name": self.stream_name,
            "finished": self.is_finished,
            "content": self.content.getvalue(),
        }

    @classmethod
    def from_json(cls, data: dict) -> "Stream":
        """Create stream from JSON data."""
        return cls(
            node_name=data["node"],
            stream_name=data["name"],
            is_finished=data["finished"],
            content=StringIO(data["content"]),
        )


class Environment:
    def __init__(self) -> None:
        self.nodes: Dict[str, Node] = {}
        self._node_counter: int = 0  # Single counter for all nodes
        self._tools: Dict[str, tuple[Callable, Callable]] = {}  # New tools dictionary
        self._streams: list[Stream] = []
        self._next_id = 1

    @property
    def streams(self) -> Sequence[Stream]:
        """Get the list of streams."""
        return self._streams

    def add_stream(self, node_name: str, stream_name: str) -> StringIO:
        """Add a new stream.

        Args:
            node_name: Name of the node this stream belongs to
            stream_name: Name of the stream

        Returns:
            The created StringIO object
        """
        stream = StringIO()
        self._streams.append(
            Stream(
                node_name=node_name,
                stream_name=stream_name,
                is_finished=False,
                content=stream,
            )
        )
        return stream

    def add_node(
        self,
        name: str,
        func: Callable[..., Any],
        deps: Optional[Sequence[Union[str, tuple[str, str]]]] = None,
        explain: Optional[str] = None,  # New parameter
    ) -> Node:
        """Add a build node with its dependencies.

        Args:
            name: Base name for the node
            func: Function to execute for this node
            deps: List of dependencies. Each dependency can be either:
                - str: node name (for default/unnamed dependencies)
                - tuple[str, str]: (node name, dependency name)
            explain: Optional explanation of what the node does

        Returns:
            The created node
        """
        self._node_counter += 1
        full_name = f"{name}.{self._node_counter}"

        # Convert all deps to tuples with Optional[str]
        normalized_deps: List[tuple[str, Optional[str]]] = []
        if deps:
            normalized_deps = [
                (dep, None) if isinstance(dep, str) else dep for dep in deps
            ]
        # Create and add node
        node = Node(name=full_name, func=func, deps=normalized_deps, explain=explain)
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
        if not node.dirty:
            return node.cache

        # Build dependencies first
        dep_results = []
        named_results: Dict[str, List[Node]] = {}

        for dep_name, dep_param in node.deps:
            dep_node = self.get_node(dep_name)
            if dep_node.dirty:
                raise ValueError(f"Dependency node '{dep_name}' is dirty")
            if dep_param is None:
                # Default dependency - add to positional args
                dep_results.append(dep_node.cache)
            else:
                # Named dependency - group by parameter name
                if dep_param not in named_results:
                    named_results[dep_param] = []
                named_results[dep_param].append(dep_node.cache)

        # Execute the node's function with all dependencies
        try:
            # Get function signature to check for env/node params
            sig = inspect.signature(node.func)
            kwargs = named_results.copy()

            # Add env and node parameters if the function accepts them
            if "env" in sig.parameters:
                kwargs["env"] = self  # type: ignore[assignment]
            if "node" in sig.parameters:
                kwargs["node"] = node  # type: ignore[assignment]

            result = node.func(dep_results, **kwargs)
        except Exception:
            print(f"Error building node '{name}'")
            print(f"Function: {node.func.__name__}")
            print(
                f"Default dependencies: "
                f"{[dep for dep, param in node.deps if param is None]}"
            )
            print(f"Named dependencies: {named_results}")
            raise

        # Since Node is frozen, we need to create a new one with updated cache
        self.nodes[name] = Node(
            name=node.name,
            func=node.func,
            deps=node.deps,
            cache=result,
            dirty=False,
            explain=node.explain,
        )
        return result

    def plan(self, target: str) -> Sequence[str]:
        """Return nodes in build order for the target.

        Args:
            target: Name of the target node

        Returns:
            List of node names in build order

        Raises:
            KeyError: If target node not found
            RuntimeError: If dependency cycle detected
        """
        if target not in self.nodes:
            raise KeyError(f"Node {target} not found")

        visited: Set[str] = set()
        build_order: List[str] = []

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
            for dep_name, _ in node.deps:
                visit(dep_name)

            visiting.remove(name)
            visiting_list.pop()
            visited.add(name)
            build_order.append(name)

        visiting: Set[str] = set()
        visiting_list: List[str] = []

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
            "\033[32m✓ built\033[0m" if not node.dirty else "\033[33m⋯ not built\033[0m"
        )

        # Print current node with explanation if it exists
        node_text = f"{indent}├── {node.name} [{status}]"
        if node.explain:
            node_text += f" ({node.explain})"
        print(node_text)

        # Track visited nodes to prevent cycles
        if node_name in visited:
            print(f"{indent}│   └── (cycle detected)")
            return
        visited.add(node_name)

        # Group dependencies by parameter name
        deps_by_param: Dict[Optional[str], List[str]] = {}
        for dep_name, param_name in node.deps:
            if param_name not in deps_by_param:
                deps_by_param[param_name] = []
            deps_by_param[param_name].append(dep_name)

        next_indent = f"{indent}│   "

        # Print default dependencies (param_name is None)
        for dep_name in deps_by_param.get(None, []):
            self.print_dependency_tree(dep_name, next_indent, visited.copy())

        # Print named dependencies grouped by parameter
        for param_name, dep_names in deps_by_param.items():
            if param_name is not None:  # Skip None group as it's already printed
                print(f"{next_indent}├── (param: {param_name})")
                param_indent = f"{next_indent}│   "
                for dep_name in dep_names:
                    self.print_dependency_tree(dep_name, param_indent, visited.copy())

        visited.remove(node_name)

    def load_node_state(
        self, node_data: Dict[str, Any], func_map: Dict[str, Callable[..., Any]]
    ) -> Node:
        """Load a node's state from JSON data.

        Args:
            node_data: Node state from JSON
            func_map: Mapping from node names to their functions

        Returns:
            The loaded node
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

        # Update counter if needed to stay above loaded node's suffix
        if "." in name:
            loaded_suffix = int(name.split(".")[-1])
            if self._node_counter <= loaded_suffix:
                self._node_counter = loaded_suffix + 1

        # Create new node with loaded state
        cache_str = node_data["cache"]
        node = Node(
            name=name,
            func=func,
            deps=node_data["deps"],
            cache=None if cache_str is None else json.loads(cache_str),
            dirty=node_data["dirty"],
            explain=node_data.get("explain"),  # Load explain field if present
        )
        self.nodes[name] = node
        return node

    def clone_path(self, start: str, end: str) -> Sequence[Node]:
        """Clone a path of nodes from start to end.

        Args:
            start: Name of starting node (can be short name without suffix)
            end: Name of ending node

        Returns:
            List of nodes in the cloned path. First element is the cloned start node,
            last element is the cloned end node. Order of other nodes is not guaranteed.
        """
        # Track which nodes have been cloned and their clones
        original_to_clone: Dict[str, str] = {}
        cloned: Set[str] = set()

        # First, get the start node
        try:
            start_node = self.get_node(start)
        except KeyError:
            start_node = self.get_node_by_base_name(start)
            start = start_node.name
        to_clone: Set[str] = {start}

        # Add start node's dependencies to clone set
        for dep_name, _ in start_node.deps:
            to_clone.add(dep_name)

        while to_clone:
            # Get next node to clone
            current_name = to_clone.pop()
            if current_name in cloned:
                continue

            current = self.get_node(current_name)
            clone = self.add_node(current_name.rsplit(".", 1)[0], current.func)
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

            if original_name == start:
                # For start node, create new list from original dependencies
                new_deps = list(original.deps)
            else:
                # For other nodes, use cloned dependencies
                new_deps = [
                    (original_to_clone[dep_name], dep_param)
                    for dep_name, dep_param in original.deps
                    if dep_name in original_to_clone
                ]

            # Create new node with dependencies
            self.nodes[clone_name] = Node(
                name=clone_name,
                func=clone.func,
                deps=new_deps,
                cache=clone.cache,
                dirty=clone.dirty,
                explain=clone.explain,
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

    def get_next_nodes(self, node: Node) -> Sequence[Node]:
        """Return list of nodes that depend on the given node."""
        next_nodes = []
        for other_node in self.nodes.values():
            # Check if node.name appears as a dependency in other_node's deps list
            if any(dep_name == node.name for dep_name, _ in other_node.deps):
                next_nodes.append(other_node)
        return next_nodes

    def add_tool(self, name: str, funcs: tuple[Callable, Callable]) -> None:
        """Add a tool with its associated functions.

        Args:
            name: Name of the tool
            funcs: Tuple of (execute_func, validate_func) for the tool
        """
        if name in self._tools:
            raise ValueError(f"Tool {name} already exists")
        self._tools[name] = funcs

    def get_tool(self, name: str) -> tuple[Callable, Callable]:
        """Get the functions associated with a tool.

        Args:
            name: Name of the tool

        Returns:
            Tuple of (execute_func, validate_func) for the tool

        Raises:
            KeyError: If tool not found
        """
        if name not in self._tools:
            raise KeyError(f"Tool {name} not found")
        return self._tools[name]

    def add_value_node(self, value: Any, explain: Optional[str] = None) -> Node:
        """Add a node that contains a pre-built value.

        Args:
            value: The value to store in the node
            explain: Optional explanation of what the value represents

        Returns:
            The created node in a built (not dirty) state
        """
        self._node_counter += 1
        full_name = f"value.{self._node_counter}"

        # Create node with the value pre-cached and marked as clean
        node = Node(
            name=full_name,
            func=lambda _: value,  # Simple function that returns the value
            deps=[],  # No dependencies
            cache=value,  # Pre-cache the value
            dirty=False,  # Mark as built
            explain=explain,
        )

        self.nodes[full_name] = node
        return node

    def add_typed_value_node(
        self, value: str, value_type: str, explain: Optional[str] = None
    ) -> Node:
        """Add a typed value node to the environment.

        Args:
            value: The value to store
            value_type: The type of the value
            explain: Optional explanation of what the value represents

        Returns:
            The created node in a built (not dirty) state
        """
        self._node_counter += 1
        full_name = f"typed_value.{self._node_counter}"

        # Create node with the value pre-cached and marked as clean
        node = Node(
            name=full_name,
            func=lambda _: (
                value,
                value_type,
            ),  # Function returns tuple of value and type
            deps=[],  # No dependencies
            cache=(value, value_type),  # Pre-cache the tuple
            dirty=False,  # Mark as built
            explain=explain,
        )

        self.nodes[full_name] = node
        return node

    def to_json(self) -> dict:
        """Convert environment to JSON-serializable dict."""
        return {
            "nodes": {name: node.to_json() for name, node in self.nodes.items()},
            "streams": [stream.to_json() for stream in self._streams],
        }

    @classmethod
    def from_json(
        cls, data: dict, func_map: Dict[str, Callable[..., Any]]
    ) -> "Environment":
        """Create environment from JSON data."""
        env = cls()

        # Load nodes
        for name, node_data in data["nodes"].items():
            env.load_node_state(node_data, func_map)

        # Load streams
        for stream_data in data.get("streams", []):
            env._streams.append(Stream.from_json(stream_data))

        return env


def mkenv() -> Environment:
    return Environment()
