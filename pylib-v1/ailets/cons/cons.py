from typing import (
    Dict,
    Any,
    Callable,
    Iterator,
    Set,
    Optional,
    TextIO,
    Sequence,
    List,
    Tuple,
)
import json

from .pipelines import get_func_map, nodelib_to_env

from .typing import Dependency, IEnvironment, NodeDescFunc, Node
from .node_runtime import NodeRuntime
from .streams import Streams, Stream


def to_basename(name: str) -> str:
    """Return the base name of a node, stripping off any numeric suffix.

    Args:
        name: The full name of the node

    Returns:
        The base name of the node without the numeric suffix
    """
    if '.' in name and name.split('.')[-1].isdigit():
        return '.'.join(name.split('.')[:-1])
    return name


class Environment(IEnvironment):
    def __init__(self) -> None:
        self.nodes: Dict[str, Node] = {}
        self._node_counter: int = 0
        self._tools: Dict[str, tuple[Callable, Callable]] = {}
        self._streams: Streams = Streams()
        self._next_id = 1
        self._aliases: Dict[str, Set[str]] = {}

    def add_node(
        self,
        name: str,
        func: Callable[..., Any],
        deps: Optional[Sequence[Dependency]] = None,
        explain: Optional[str] = None,  # New parameter
    ) -> Node:
        """Add a build node with its dependencies.

        Args:
            name: Base name for the node
            func: Function to execute for this node
            deps: List of dependencies
            explain: Optional explanation of what the node does

        Returns:
            The created node
        """
        self._node_counter += 1
        full_name = f"{name}.{self._node_counter}"
        node = Node(name=full_name, func=func, deps=list(deps or []), explain=explain)
        self.nodes[full_name] = node
        return node

    def get_node(self, name: str) -> Node:
        """Get a node by name. Does not build."""
        if name in self._aliases:
            aliases = self._aliases[name]
            if len(aliases) > 0:
                assert len(aliases) == 1, f"Ambiguous alias: {name} to {aliases}"
                name = next(iter(aliases))
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

    def build_node_alone(self, name: str) -> None:
        """Build a node. Does not build its dependencies."""
        node = self.get_node(name)

        in_streams: Dict[Optional[str], List[Stream]] = {}

        for dep in self.iter_deps(name):
            dep_node_name, dep_name, dep_stream_name = (
                dep.source,
                dep.name,
                dep.stream,
            )
            if not self.is_node_built(dep_node_name):
                raise ValueError(f"Dependency node '{dep_node_name}' is not built")

            dep_stream = self._streams.get(dep_node_name, dep_stream_name)
            if not dep_stream.is_finished:
                raise ValueError(
                    f"Stream '{dep_stream_name}' for node "
                    f"'{dep_node_name}' is not finished"
                )

            if dep_name not in in_streams:
                in_streams[dep_name] = []
            in_streams[dep_name].append(dep_stream)

        runtime = NodeRuntime(self, in_streams, node.name)

        # Execute the node's function with all dependencies
        try:
            node.func(runtime)
        except Exception:
            print(f"Error building node '{name}'")
            print(f"Function: {node.func.__name__}")
            print("Dependencies:")
            for dep in node.deps:
                print(f"  {dep.source} ({dep.stream}) -> {dep.name}")
            raise

    def build_target(
        self,
        target: str,
        one_step: bool = False,
    ) -> None:
        """Build nodes in order.

        Args:
            env: Environment to build in
            target: Target node to build
            one_step: If True, build only one step and exit
        """

        # Get initial plan
        plan = self.plan(target)
        current_node_count = len(self.nodes)

        while True:
            next_node = None
            for node_name in plan:
                node = self.get_node(node_name)
                if not self.is_node_built(node_name):
                    next_node = node
                    break
            # If no dirty nodes, we're done
            if next_node is None:
                break

            # Build the node
            self.build_node_alone(next_node.name)

            # Check if number of nodes changed
            new_node_count = len(self.nodes)
            if new_node_count != current_node_count:
                # Recalculate plan
                plan = self.plan(target)
                current_node_count = new_node_count

            if one_step:  # Exit after building one node if requested
                break

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
            for dep in self.iter_deps(name):
                visit(dep.source)

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
        self,
        node_name: str,
        indent: str = "",
        visited: Optional[Set[str]] = None,
        stream_name: Optional[str] = None,
    ) -> None:
        """Print a tree showing node dependencies and build status.

        Args:
            node_name: Name of the node to print
            indent: Current indentation string (used for recursion)
            visited: Set of visited nodes to prevent cycles
            stream_name: Optional stream name to display
        """
        if visited is None:
            visited = set()

        node = self.get_node(node_name)
        status = (
            "\033[32m✓ built\033[0m"
            if self.is_node_built(node_name)
            else "\033[33m⋯ not built\033[0m"
        )

        # Print current node with explanation if it exists
        display_name = node.name
        if stream_name is not None:
            display_name = f"{display_name}.{stream_name}"

        node_text = f"{indent}├── {display_name} [{status}]"
        if node.explain:
            node_text += f" ({node.explain})"
        print(node_text)

        # Track visited nodes to prevent cycles
        if node_name in visited:
            print(f"{indent}│   └── (cycle detected)")
            return
        visited.add(node_name)

        # Group dependencies by parameter name
        deps_by_param: Dict[Optional[str], List[Tuple[str, Optional[str]]]] = {}
        for dep in self.iter_deps(node_name):
            if dep.name not in deps_by_param:
                deps_by_param[dep.name] = []
            deps_by_param[dep.name].append((dep.source, dep.stream))

        next_indent = f"{indent}│   "

        # Print default dependencies (param_name is None)
        for dep_name, stream_name in deps_by_param.get(None, []):
            self.print_dependency_tree(
                dep_name, next_indent, visited.copy(), stream_name
            )

        # Print named dependencies grouped by parameter
        for param_name, dep_names in deps_by_param.items():
            if param_name is not None:  # Skip None group as it's already printed
                print(f"{next_indent}├── (param: {param_name})")
                param_indent = f"{next_indent}│   "
                for dep_name, stream_name in dep_names:
                    self.print_dependency_tree(
                        dep_name, param_indent, visited.copy(), stream_name
                    )

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
        node = Node(
            name=name,
            func=func,
            deps=[Dependency.from_json(dep) for dep in node_data["deps"]],
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
        for dep in self.iter_deps(start):
            to_clone.add(dep.source)

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
                new_deps = list(self.iter_deps(original_name))
            else:
                # For other nodes, use cloned dependencies
                new_deps = [
                    Dependency(
                        name=dep.name,
                        source=original_to_clone[dep.source],
                        stream=dep.stream,
                    )
                    for dep in original.deps
                    if dep.source in original_to_clone
                ]

            # Create new node with dependencies
            self.nodes[clone_name] = Node(
                name=clone_name,
                func=clone.func,
                deps=new_deps,
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
            if any(dep.source == node.name for dep in self.iter_deps(other_node.name)):
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

    def add_typed_value_node(
        self, value: str, value_type: str, explain: Optional[str] = None
    ) -> Node:
        """Add a typed value node to the environment.

        Args:
            value: The value to store
            value_type: The type of the value
            explain: Optional explanation of what the value represents

        """
        self._node_counter += 1
        full_name = f"typed_value.{self._node_counter}"

        node = Node(
            name=full_name,
            func=lambda _: (
                value,
                value_type,
            ),  # Function returns tuple of value and type
            deps=[],  # No dependencies
            explain=explain,
        )

        self.nodes[full_name] = node

        # Add streams for value and type
        value_stream = self._streams.create(full_name, None)
        value_stream.content.write(value)
        value_stream.is_finished = True

        type_stream = self._streams.create(full_name, "type")
        type_stream.content.write(value_type)
        type_stream.is_finished = True

        return node

    def to_json(self, f: TextIO) -> None:
        """Convert environment to JSON-serializable dict."""
        # Save nodes
        for node in self.nodes.values():
            json.dump(node.to_json(), f, indent=2)
            f.write("\n")

        self._streams.to_json(f)

        for alias, names in self._aliases.items():
            json.dump({"alias": alias, "names": list(names)}, f, indent=2)
            f.write("\n")

    @classmethod
    def from_json(cls, f: TextIO, nodelib: Sequence[NodeDescFunc]) -> "Environment":
        """Create environment from JSON data."""
        env = cls()

        nodelib_to_env(env, nodelib)
        func_map = get_func_map(nodelib)

        content = f.read()
        decoder = json.JSONDecoder()
        pos = 0

        # Decode multiple JSON objects from the content
        while pos < len(content):
            # Skip whitespace
            while pos < len(content) and content[pos].isspace():
                pos += 1
            if pos >= len(content):
                break

            # Decode next object
            try:
                obj_data, pos = decoder.raw_decode(content, pos)
                if "deps" in obj_data:
                    env.load_node_state(obj_data, func_map)
                elif "is_finished" in obj_data:
                    env._streams.add_stream_from_json(obj_data)
                elif "alias" in obj_data:
                    env._aliases[obj_data["alias"]] = set(obj_data["names"])
                else:
                    raise ValueError(f"Unknown object data: {obj_data}")
            except json.JSONDecodeError as e:
                print(f"Error decoding JSON at position {pos}: {e}")
                raise

        return env

    def create_new_stream(self, node_name: str, stream_name: Optional[str]) -> Stream:
        return self._streams.create(node_name, stream_name)

    def close_stream(self, stream: Stream) -> None:
        stream.close()

    def is_node_built(self, node_name: str) -> bool:
        """Check if a node has been built by checking if it has any finished streams.

        Args:
            node_name: Name of the node to check

        Returns:
            True if the node has at least one finished stream, False otherwise
        """
        return any(
            stream.is_finished
            for stream in self._streams._streams
            if stream.node_name == node_name
        )

    def alias(self, alias: str, node_name: str) -> None:
        """Associate an alias with a node.

        Args:
            alias: The alias name to create
            node_name: The node name to associate with the alias

        Raises:
            KeyError: If the node name doesn't exist
        """
        # Verify node exists
        if node_name not in self.nodes:
            raise KeyError(f"Node {node_name} not found")

        # Create or update alias
        if alias not in self._aliases:
            self._aliases[alias] = {node_name}
        else:
            self._aliases[alias].add(node_name)

    def get_nodes_by_alias(self, alias: str) -> Set[Node]:
        """Get all nodes associated with an alias.

        Args:
            alias: The alias to look up

        Returns:
            Set of nodes associated with the alias. Returns empty set if alias not
            found.
        """
        if alias not in self._aliases:
            return set()

        return {self.nodes[name] for name in self._aliases[alias]}

    def iter_deps(self, name: str) -> Iterator[Dependency]:
        """Iterate through dependencies of a node, resolving alias dependencies.

        Args:
            name: Name of the node

        Yields:
            Dependencies of the node, with alias dependencies resolved to concrete nodes
        """
        node = self.get_node(name)
        for dep in node.deps:
            # If dependency source is an alias, yield a dependency for each aliased node
            if dep.source in self._aliases:
                for aliased_node_name in self._aliases[dep.source]:
                    yield Dependency(
                        source=aliased_node_name, name=dep.name, stream=dep.stream
                    )
            else:
                yield dep
