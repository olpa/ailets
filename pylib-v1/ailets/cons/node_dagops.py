from typing import List, Optional, Sequence, Dict, Set, Tuple

from ailets.cons.pipelines import instantiate_with_deps

from .util import to_basename
from .typing import (
    BeginEnd,
    Dependency,
    IEnvironment,
    INodeDagops,
    INodeRegistry,
    INodeRuntime,
    Node,
)


class NodeDagops(INodeDagops):
    def __init__(self, env: IEnvironment, nodereg: INodeRegistry, node: INodeRuntime):
        self._env = env
        self._nodereg = nodereg
        self._node = node

    def depend(self, target: str, source: Sequence[Dependency]) -> None:
        self._env.depend(target, source)

    def clone_path(self, begin: str, end: str) -> BeginEnd:
        """Clone a path from begin to end.

        begin should be the base name
        end should be the full name
        """

        def get_next_nodes(node_name: str) -> Sequence[Node]:
            next_nodes = []
            for other_node in self._env.get_nodes():
                if any(
                    dep.source == node_name
                    for dep in self._env.iter_deps(other_node.name)
                ):
                    next_nodes.append(other_node)
            return next_nodes

        def find_nodes_to_clone(start_name: str) -> Set[str]:
            """Find all nodes between start node and end node that need to be cloned.

            Args:
                start_name: Name of node to start traversal from

            Returns:
                Set of node names that need to be cloned
            """
            visited: Set[str] = set()
            to_clone: Set[str] = set()
            to_visit: Set[str] = {start_name}

            while to_visit:
                current_name = to_visit.pop()
                if current_name in visited:
                    continue

                visited.add(current_name)
                to_clone.add(current_name)

                if current_name != end:
                    next_nodes = get_next_nodes(current_name)
                    for next_node in next_nodes:
                        if next_node.name not in visited:
                            to_visit.add(next_node.name)

            return to_clone

        start_node_name = self.get_upstream_node(begin)
        to_clone = find_nodes_to_clone(start_node_name)
        original_to_clone: Dict[str, str] = {}

        #
        # Clone each node
        #
        for node_name in to_clone:
            original_node = self._env.get_node(node_name)
            original_node_name = to_basename(original_node.name)

            # Create new node with same function but no dependencies yet
            cloned_node = self._env.add_node(
                name=original_node_name,  # Will get auto-numbered suffix
                deps=[],  # Dependencies added later
                func=original_node.func,
                explain=original_node.explain,
            )

            # Store mapping from original to cloned name
            original_to_clone[node_name] = cloned_node.name

        #
        # Recreate dependencies between cloned nodes
        #
        for node_name in to_clone:
            original_node = self._env.get_node(node_name)
            cloned_name = original_to_clone[node_name]

            # For each dependency of the original node
            for dep in self._env.iter_deps(node_name):
                # If dependency source was cloned, point to cloned version
                if dep.source in original_to_clone:
                    cloned_dep = Dependency(
                        source=original_to_clone[dep.source],
                        name=dep.name,
                        stream=dep.stream,
                    )
                    self._env.depend(cloned_name, [cloned_dep])
                else:
                    # Otherwise keep original dependency
                    self._env.depend(cloned_name, [dep])

        #
        # Add dependencies for nodes that depended on end node
        #
        cloned_end_name = original_to_clone[end]
        for node in self._env.get_nodes():
            for dep in self._env.iter_deps(node.name):
                if dep.source == end:
                    cloned_dep = Dependency(
                        source=cloned_end_name, name=dep.name, stream=dep.stream
                    )
                    self._env.depend(node.name, [cloned_dep])

        cloned_source_name = original_to_clone[start_node_name]
        return BeginEnd(begin=cloned_source_name, end=cloned_end_name)

    def get_upstream_node(self, begin: str) -> str:
        """Find the upstream node with basename 'begin' by traversing back from 'end'.

        Args:
            begin: The base name to search for
            end: The full name of the node to start traversing from

        Returns:
            The full name of the found upstream node

        Raises:
            ValueError: If no upstream node with basename 'begin' is found
        """
        visited: Set[str] = set()
        to_visit: Set[str] = {self._node.get_name()}

        while to_visit:
            current_node_name = to_visit.pop()
            visited.add(current_node_name)

            if to_basename(current_node_name) == begin:
                # Found the requested start node
                return current_node_name
            # Add dependencies to visit
            for dep in self._env.iter_deps(current_node_name):
                if dep.source not in visited:
                    to_visit.add(dep.source)

        raise ValueError(
            f"Start node {begin} not found in far dependencies of "
            f"{current_node_name}"
        )

    def add_typed_value_node(
        self, value: str, value_type: str, explain: Optional[str] = None
    ) -> str:
        node = self._env.add_typed_value_node(value, value_type, explain)
        return node.name

    def add_node(
        self,
        name: str,
        deps: Optional[Sequence[Dependency]] = None,
        explain: Optional[str] = None,
    ) -> str:
        basename = to_basename(name)
        existing_node = self._env.get_node_by_base_name(basename)
        node = self._env.add_node(name, existing_node.func, deps, explain)
        return node.name

    def clone_node(self, node_name: str) -> str:
        return self._env.clone_node(node_name)

    def instantiate_tool(self, tool_name: str, tool_input_node_name: str) -> str:
        return self._env.instantiate_tool(
            self._nodereg, tool_name, tool_input_node_name
        )

    def instantiate_with_deps(self, target: str, aliases: dict[str, str]) -> str:
        return instantiate_with_deps(self._env, self._nodereg, target, aliases)

    def alias(self, alias: str, node_name: Optional[str]) -> None:
        self._env.alias(alias, node_name)

    def expand_alias(self, alias: str) -> Sequence[str]:
        return self._env.expand_alias(alias)

    def invalidate(self, alias: str, old_nodes: Sequence[str]) -> None:
        # Build reverse dependency map
        nodedeps_reverse: Dict[str, Set[str]] = {}
        for node_name in self._env.nodes:
            for dep in self._env.iter_deps(node_name):
                if dep.source not in nodedeps_reverse:
                    nodedeps_reverse[dep.source] = set()
                nodedeps_reverse[dep.source].add(node_name)

        # Build reverse alias map: node -> aliases that point to it
        aliases_reverse: Dict[str, Set[str]] = {}
        for alias, nodes in self._env._aliases.items():
            for node in nodes:
                if node not in aliases_reverse:
                    aliases_reverse[node] = set()
                aliases_reverse[node].add(alias)

        # Create maps to track old->new mappings
        # Tuple is (new_name, defunc_name)
        old_to_new_names: Dict[str, Tuple[str, str]] = {}

        # Create queues for processing
        node_queue: List[str] = []
        alias_queue: List[str] = []

        def add_downstream_to_queue(names: Sequence[str]) -> None:
            for name in names:
                if name in old_to_new_names:
                    continue
                if name in self._env._aliases:
                    alias_queue.append(name)
                elif name in self._env.nodes:
                    node_queue.append(name)
            else:
                raise ValueError(f"Unknown name: {name}")

        def fix_deplist_inplace(deplist: List[str | Dependency]) -> None:
            i = 0
            while i < len(deplist):
                item = deplist[i]
                if isinstance(item, str) and item in old_to_new_names:
                    (new_name, defunc_name) = old_to_new_names[item]
                    deplist[i] = new_name
                    deplist.insert(i, defunc_name)
                    i += 2
                elif isinstance(item, Dependency) and item.source in old_to_new_names:
                    (new_name, defunc_name) = old_to_new_names[item.source]
                    deplist[i] = Dependency(**item.asdict(), source=new_name)
                    deplist.insert(i, Dependency(**item.asdict(), source=defunc_name))
                    i += 2
                else:
                    i += 1

        def collect_node(node_name: str) -> None:
            if node_name in old_to_new_names:
                return

            defunc_name = f"defunc.{node_name}"
            new_node_name = self._env.get_next_name(node_name)
            old_to_new_names[node_name] = (new_node_name, defunc_name)

            node = self._env.nodes[node_name]
            del self._env.nodes[node_name]
            defunc_dep_list = list(node.deps)
            self._env.nodes[new_node_name] = Node(**node.asdict(), name=new_node_name)
            self._env.nodes[defunc_name] = Node(
                **node.asdict(), deps=defunc_dep_list, name=defunc_name
            )

            add_downstream_to_queue(nodedeps_reverse.get(node_name, []))

        def collect_alias(alias_name: str) -> None:
            if alias_name in old_to_new_names:
                return

            defunc_name = f"defunc.{alias_name}"
            new_alias_name = self._env.get_next_name(alias_name)
            old_to_new_names[alias_name] = (new_alias_name, defunc_name)

            new_alias_list = self._env._aliases[alias_name]
            del self._env._aliases[alias_name]
            defunc_alias_list = list(new_alias_list)
            self._env._aliases[defunc_name] = defunc_alias_list
            self._env.aliases[new_alias_name] = new_alias_list

            add_downstream_to_queue(aliases_reverse.get(alias_name, []))

        #
        # Pass 1: Collect affected aliased and nodes
        #
        while node_queue or alias_queue:
            # Process next node if available
            if node_queue:
                next_node = node_queue.pop(0)
                collect_node(next_node)

            elif alias_queue:
                next_alias = alias_queue.pop(0)
                collect_alias(next_alias)

        print(old_to_new_names)  # FIXME: Remove
        raise ValueError("Not implemented")
