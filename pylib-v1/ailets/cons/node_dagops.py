from typing import Optional, Sequence, Dict, Set, Tuple

from ailets.cons.cons import to_basename
from .typing import INodeDagops, IEnvironment, INodeRuntime, Dependency, BeginEnd, Node


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
        def find_start_node() -> Node:
            # Find start node by traversing back through dependencies
            current_node = end
            visited: Set[str] = set()
            to_visit: Set[str] = {current_node}
            
            while True:
                if to_basename(current_node.name) == begin:
                    # Found the requested start node
                    return current_node
                # Add dependencies to visit
                for dep in current_node.deps:
                    if dep.source not in visited:
                        to_visit.add(dep.source)
                
                if not to_visit:
                    # No more nodes to visit - we've found the start
                    break
                    
                # Visit next node
                current_name = to_visit.pop()
                current_node = self._env.get_node(current_name)
                visited.add(current_name)
                    
            raise ValueError(f"Start node {begin} not found in far dependencies of {current_node}")

        def get_next_nodes(self, node_name: str) -> Sequence[Node]:
            next_nodes = []
            for other_node in self._env.nodes.values():
                if any(dep.source == node_name for dep in self._env.iter_deps(other_node.name)):
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
                if current_name != end:
                    to_clone.add(current_name)
                    
                    # Add next nodes to visit
                    next_nodes = get_next_nodes(current_name)
                    for next_node in next_nodes:
                        if next_node.name not in visited:
                            to_visit.add(next_node.name)
                            
            return to_clone

        start_node = find_start_node()
        to_clone = find_nodes_to_clone(start_node.name)
        # Clone nodes, keeping map of original to cloned names
        original_to_clone: Dict[str, str] = {}
        
        #
        # Clone each node
        #
        for node_name in to_clone:
            original_node = self._env.get_node(node_name)
            
            # Create new node with same function but no dependencies yet
            cloned_node = self._env.add_node(
                name=original_node.name,  # Will get auto-numbered suffix
                deps=[],  # Dependencies added later
                explain=original_node.explain
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
                        stream=dep.stream
                    )
                    self._env.depend(cloned_name, [cloned_dep])
                else:
                    # Otherwise keep original dependency
                    self._env.depend(cloned_name, [dep])

        #
        # Add dependencies for source node
        #
        cloned_source = original_to_clone[start_node.name]
        for dep in self._env.iter_deps(start_node.name):
            self._env.depend(cloned_source, [dep])
        #
        # Add dependencies for nodes that depended on end node
        #
        # For each node in the environment
        cloned_end_name = original_to_clone[end]
        for node_name, node in self._env.nodes.items():
            for dep in self._env.iter_deps(node_name):
                if dep.source == end:
                    cloned_dep = Dependency(
                        source=cloned_end_name,
                        name=dep.name,
                        stream=dep.stream
                    )
                    self._env.depend(node_name, [cloned_dep])


        return BeginEnd(
            begin=cloned_source.name,
            end=cloned_end_name
        )

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
