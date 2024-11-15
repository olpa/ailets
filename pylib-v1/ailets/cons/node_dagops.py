from typing import Optional, Sequence, Dict, Set

from .util import to_basename
from .typing import IEnvironment, INodeDagops, INodeRuntime, Dependency, BeginEnd, Node


class NodeDagops(INodeDagops):
    def __init__(self, env: IEnvironment, node: INodeRuntime):
        self._env = env
        self._node = node

    def depend(self, target: str, source: Sequence[Dependency]) -> None:
        raise NotImplementedError

    def clone_path(self, begin: str, end: str) -> BeginEnd:
        """Clone a path from begin to end.

        begin should be the base name
        end should be the full name
        """

        def find_start_node() -> str:
            # Find start node by traversing back through dependencies
            visited: Set[str] = set()
            to_visit: Set[str] = {end}

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

        start_node_name = find_start_node()
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

    def add_typed_value_node(
        self, value: str, value_type: str, explain: Optional[str] = None
    ) -> str:
        raise NotImplementedError

    def add_node(
        self,
        name: str,
        deps: Optional[Sequence[Dependency]] = None,
        explain: Optional[str] = None,
    ) -> str:
        raise NotImplementedError

    def instantiate_tool(self, tool_name: str) -> BeginEnd:
        raise NotImplementedError
