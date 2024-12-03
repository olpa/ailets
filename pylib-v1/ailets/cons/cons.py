from typing import (
    Awaitable,
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

from .plugin import NodeRegistry

from .atyping import (
    Dependency,
    IEnvironment,
    INodeRegistry,
    INodeRuntime,
    Node,
    IStream,
)
from .node_runtime import NodeRuntime
from .streams import Streams
from .util import to_basename


class Environment(IEnvironment):
    def __init__(self) -> None:
        self.nodes: Dict[str, Node] = {}
        self._for_env_stream: Dict[str, Any] = {}
        self._streams: Streams = Streams()
        self._seqno: int = 1
        self._aliases: Dict[str, List[str]] = {}

    def privates_for_dagops_friend(
        self,
    ) -> Tuple[Dict[str, Node], Dict[str, List[str]]]:
        """Return private nodes and aliases for NodeDagops friend class."""
        return self.nodes, self._aliases

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
        full_name = self.get_next_name(name)
        node = Node(name=full_name, func=func, deps=list(deps or []), explain=explain)
        self.nodes[full_name] = node
        return node

    def _resolve_alias(self, name: str) -> str:
        if name in self._aliases:
            aliases = self._aliases[name]
            if len(aliases) > 0:
                assert len(aliases) == 1, f"Ambiguous alias: {name} to {aliases}"
                return next(iter(aliases))
        return name

    def has_node(self, node_name: str) -> bool:
        return node_name in self.nodes or node_name in self._aliases

    def get_node(self, name: str) -> Node:
        """Get a node by name. Does not build."""
        name = self._resolve_alias(name)
        if name not in self.nodes:
            raise KeyError(f"Node {name} not found")
        return self.nodes[name]

    def depend(self, target: str, deps: Sequence[Dependency]) -> None:
        """Add dependencies to a node.

        Args:
            target: Name of node to add dependencies to
            deps: Dependencies to add
        """
        if target in self._aliases:
            self._aliases[target].extend(dep.source for dep in deps)
            return

        node = self.get_node(target)
        self.nodes[target] = Node(
            name=node.name,
            func=node.func,
            deps=list(node.deps) + list(deps),
            explain=node.explain,
        )

    async def build_node_alone(self, nodereg: INodeRegistry, name: str) -> None:
        """Build a node. Does not build its dependencies."""
        node = self.get_node(name)

        deps = list(self.iter_deps(name))
        for dep in deps:
            dep_name = dep.source
            if not self.is_node_built(dep_name):
                raise ValueError(f"Dependency node '{dep_name}' is not built")

        runtime = NodeRuntime(self, nodereg, self._streams, node.name, deps)

        # Execute the node's function with all dependencies
        try:
            await node.func(runtime)
        except Exception:
            print(f"Error building node '{name}'")
            print(f"Function: {node.func.__name__}")
            print("Dependencies:")
            for dep in node.deps:
                print(f"  {dep.source} ({dep.stream}) -> {dep.name}")
            raise

    async def build_target(
        self,
        nodereg: INodeRegistry,
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
            await self.build_node_alone(nodereg, next_node.name)

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
            target = self._resolve_alias(target)
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
        if node_name.startswith("defunc."):
            status = "\033[90mdefunc\033[0m"
        else:
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

    def load_node_state(self, node_data: Dict[str, Any], nodereg: NodeRegistry) -> Node:
        """Load a node's state from JSON data.

        Args:
            node_data: Node state from JSON

        Returns:
            The loaded node
        """
        name = node_data["name"]

        # Try to get function from map, if not found and name has a number suffix,
        # try without the suffix
        func: Callable[[INodeRuntime], Awaitable[None]]
        base_name = to_basename(name)
        if base_name.startswith("defunc."):
            base_name = base_name[7:]
        if base_name == "value":
            # Special case for typed value nodes
            async def func(
                _: INodeRuntime,
            ) -> None: ...

        else:
            node_desc = nodereg.nodes.get(base_name)
            if node_desc is None:
                raise KeyError(f"No function registered for node: {name} ({base_name})")
            func = node_desc.func

        # Update counter if needed to stay above loaded node's suffix
        if "." in name:
            loaded_suffix = int(name.split(".")[-1])
            if self._seqno <= loaded_suffix:
                self._seqno = loaded_suffix + 1

        # Create new node with loaded state
        node = Node(
            name=name,
            func=func,
            deps=[Dependency.from_json(dep) for dep in node_data["deps"]],
            explain=node_data.get("explain"),  # Load explain field if present
        )
        self.nodes[name] = node
        return node

    def add_value_node(self, value: bytes, explain: Optional[str] = None) -> Node:
        """Add a typed value node to the environment.

        Args:
            value: The value to store
            explain: Optional explanation of what the value represents

        """
        full_name = self.get_next_name("value")

        async def async_dummy(runtime: INodeRuntime) -> None: ...

        node = Node(
            name=full_name,
            func=async_dummy,
            deps=[],  # No dependencies
            explain=explain,
        )

        self.nodes[full_name] = node

        # Add streams for value and type
        self._streams.create(full_name, None, value, is_closed=True)

        return node

    async def to_json(self, f: TextIO) -> None:
        """Convert environment to JSON-serializable dict."""
        # Save nodes
        for node in self.nodes.values():
            json.dump(node.to_json(), f, indent=2)
            f.write("\n")

        await self._streams.to_json(f)

        json.dump({"env": self._for_env_stream}, f, indent=2)
        f.write("\n")

        for alias, names in self._aliases.items():
            json.dump({"alias": alias, "names": list(names)}, f, indent=2)
            f.write("\n")

    @classmethod
    async def from_json(cls, f: TextIO, nodereg: NodeRegistry) -> "Environment":
        """Create environment from JSON data."""
        env = cls()

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
                    env.load_node_state(obj_data, nodereg)
                elif "is_closed" in obj_data:
                    await env._streams.add_stream_from_json(obj_data)
                elif "alias" in obj_data:
                    env._aliases[obj_data["alias"]] = obj_data["names"]
                elif "env" in obj_data:
                    env._for_env_stream.update(obj_data["env"])
                else:
                    raise ValueError(f"Unknown object data: {obj_data}")
            except json.JSONDecodeError as e:
                print(f"Error decoding JSON at position {pos}: {e}")
                raise

        return env

    def create_new_stream(self, node_name: str, stream_name: Optional[str]) -> IStream:
        return self._streams.create(node_name, stream_name)

    def is_node_built(self, node_name: str) -> bool:
        """Check if a node has been built by checking if it has any finished streams.

        Args:
            node_name: Name of the node to check

        Returns:
            True if the node has at least one finished stream, False otherwise
        """
        return any(
            stream.is_closed()
            for stream in self._streams._streams
            if stream.node_name == node_name
        )

    def alias(self, alias: str, node_name: Optional[str]) -> None:
        """Associate an alias with a node.

        Args:
            alias: The alias name to create
            node_name: The node name to associate with the alias

        Raises:
            KeyError: If the node name doesn't exist
        """
        if node_name is None:
            if alias not in self._aliases:
                self._aliases[alias] = []
            return

        # Verify node exists
        if node_name not in self.nodes:
            raise KeyError(f"Node {node_name} not found")

        # Create or update alias
        if alias not in self._aliases:
            self._aliases[alias] = [node_name]
        else:
            self._aliases[alias].append(node_name)

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
        seen_deps = set()  # Track seen dependencies to avoid duplicates

        for dep in node.deps:
            # If dependency source is an alias, yield a dependency for each aliased node
            if dep.source in self._aliases:
                # Recursively expand aliases
                def expand_alias_deps(
                    alias_name: str, seen_aliases: Set[str]
                ) -> Iterator[str]:
                    if alias_name in seen_aliases:
                        return  # Prevent infinite recursion
                    seen_aliases.add(alias_name)

                    for aliased_name in self._aliases[alias_name]:
                        if aliased_name in self._aliases:
                            yield from expand_alias_deps(aliased_name, seen_aliases)
                        else:
                            yield aliased_name

                for aliased_node_name in expand_alias_deps(dep.source, set()):
                    dep_key = (aliased_node_name, dep.name, dep.stream)
                    if dep_key not in seen_deps:
                        seen_deps.add(dep_key)
                        yield Dependency(
                            source=aliased_node_name, name=dep.name, stream=dep.stream
                        )
            else:
                dep_key = (dep.source, dep.name, dep.stream)
                if dep_key not in seen_deps:
                    seen_deps.add(dep_key)
                    yield dep

    def get_next_seqno(self) -> int:
        self._seqno += 1
        return self._seqno

    def get_next_name(self, full_name: str) -> str:
        """Get the next name in the sequence."""
        seqno = self.get_next_seqno()
        another_name = f"{to_basename(full_name)}.{seqno}"
        return another_name

    def update_for_env_stream(self, params: Dict[str, Any]) -> None:
        self._for_env_stream.update(params)

    def get_env_stream(self) -> IStream:
        return self._streams.make_env_stream(self._for_env_stream)

    def get_fs_output_streams(self) -> Sequence[IStream]:
        return self._streams.get_fs_output_streams()
