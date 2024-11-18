from typing import Optional, Sequence, Dict, Set

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
