from typing import (
    Dict,
    Any,
    Callable,
    Iterator,
    Set,
    Optional,
    Sequence,
    List,
    Tuple,
)

from ailets.atyping import (
    Dependency,
    IDagops,
    IProcesses,
    IPiper,
    Node,
)
from ailets.cons.notification_queue import INotificationQueue
from ailets.cons.util import to_basename
from ailets.cons.seqno import Seqno
from ailets.cons.value_node import (
    add_value_node as _add_value_node,
    add_open_value_node as _add_open_value_node,
)


class Dagops(IDagops):
    def __init__(self, seqno: Seqno) -> None:
        self.nodes: Dict[str, Node] = {}
        self.seqno: Seqno = seqno
        self.aliases: Dict[str, List[str]] = {}

    def privates_for_dagops_friend(
        self,
    ) -> Tuple[Dict[str, Node], Dict[str, List[str]]]:
        """Return private nodes and aliases for NodeDagops friend class."""
        return self.nodes, self.aliases

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
        if name in self.aliases:
            aliases = self.aliases[name]
            if len(aliases) > 0:
                assert len(aliases) == 1, f"Ambiguous alias: {name} to {aliases}"
                return next(iter(aliases))
        return name

    def has_node(self, node_name: str) -> bool:
        return node_name in self.nodes or node_name in self.aliases

    def get_node(self, name: str) -> Node:
        """Get a node by name. Does not build."""
        name = self._resolve_alias(name)
        if name not in self.nodes:
            raise KeyError(f"Node {name} not found")
        return self.nodes[name]

    def get_node_names(self) -> Sequence[str]:
        return list(self.nodes.keys())

    def depend(self, target: str, deps: Sequence[Dependency]) -> None:
        """Add dependencies to a node.

        Args:
            target: Name of node to add dependencies to
            deps: Dependencies to add
        """
        if target in self.aliases:
            self.aliases[target].extend(dep.source for dep in deps)
            return

        node = self.get_node(target)
        self.nodes[target] = Node(
            name=node.name,
            func=node.func,
            deps=list(node.deps) + list(deps),
            explain=node.explain,
        )

    def add_value_node(
        self,
        value: bytes,
        piper: IPiper,
        processes: IProcesses,
        explain: Optional[str] = None,
    ) -> Node:
        """Add a typed value node to the environment."""
        return _add_value_node(self, value, piper, processes, explain)

    def create_open_value_node(
        self,
        piper: IPiper,
        processes: IProcesses,
        notification_queue: INotificationQueue,
        explain: Optional[str] = None,
    ) -> Node:
        """Create a value node without closing the pipe or marking as completed."""
        return _add_open_value_node(self, piper, notification_queue, explain)

    def alias(self, alias: str, node_name: Optional[str]) -> None:
        """Associate an alias with a node.

        Args:
            alias: The alias name to create
            node_name: The node name to associate with the alias

        Raises:
            KeyError: If the node name doesn't exist
        """
        if node_name is None:
            if alias not in self.aliases:
                self.aliases[alias] = []
            return

        # Verify node exists
        if node_name not in self.nodes:
            raise KeyError(f"Node {node_name} not found")

        # Create or update alias
        if alias not in self.aliases:
            self.aliases[alias] = [node_name]
        else:
            self.aliases[alias].append(node_name)

    def get_nodes_by_alias(self, alias: str) -> Set[Node]:
        """Get all nodes associated with an alias.

        Args:
            alias: The alias to look up

        Returns:
            Set of nodes associated with the alias. Returns empty set if alias not
            found.
        """
        if alias not in self.aliases:
            return set()

        return {self.nodes[name] for name in self.aliases[alias]}

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
            if dep.source in self.aliases:
                # Recursively expand aliases
                def expand_alias_deps(
                    alias_name: str, seen_aliases: Set[str]
                ) -> Iterator[str]:
                    if alias_name in seen_aliases:
                        return  # Prevent infinite recursion
                    seen_aliases.add(alias_name)

                    for aliased_name in self.aliases[alias_name]:
                        if aliased_name in self.aliases:
                            yield from expand_alias_deps(aliased_name, seen_aliases)
                        else:
                            yield aliased_name

                for aliased_node_name in expand_alias_deps(dep.source, set()):
                    dep_key = (aliased_node_name, dep.name, dep.slot)
                    if dep_key not in seen_deps:
                        seen_deps.add(dep_key)
                        yield Dependency(
                            source=aliased_node_name, name=dep.name, slot=dep.slot
                        )
            else:
                dep_key = (dep.source, dep.name, dep.slot)
                if dep_key not in seen_deps:
                    seen_deps.add(dep_key)
                    yield dep

    def get_next_name(self, full_name: str) -> str:
        """Get the next name in the sequence."""
        seqno = self.seqno.next_seqno()
        another_name = f"{to_basename(full_name)}.{seqno}"
        return another_name

    def hash_of_nodenames(self) -> int:
        """Generate a hash based on the names of nodes in the DAG."""
        # Sort node names for deterministic hashing
        sorted_names = sorted(self.nodes.keys())
        # Use tuple since lists aren't hashable
        return hash(tuple(sorted_names))
