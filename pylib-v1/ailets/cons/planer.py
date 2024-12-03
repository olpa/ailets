from typing import AsyncIterator, Dict, Set, Optional
from .atyping import IEnvironment, Node

class BuildPlaner:
    def __init__(self, env: IEnvironment) -> None:
        self._env = env
        self._dirty_nodes: Set[str] = set()
        self._in_progress: Set[str] = set()
        self._completed: Set[str] = set()
        self._target: Optional[str] = None

    def set_target(self, target: str) -> None:
        """Set the target node to build towards."""
        self._target = target
        self._recalculate_dirty()

    def notify_node_completed(self, node_name: str) -> None:
        """Notify that a node has completed building."""
        if node_name in self._in_progress:
            self._in_progress.remove(node_name)
            self._completed.add(node_name)

    def notify_graph_changed(self) -> None:
        """Notify that the dependency graph has changed."""
        self._recalculate_dirty()

    def _recalculate_dirty(self) -> None:
        """Recalculate which nodes need to be built."""
        if not self._target:
            return

        # Start fresh with only completed nodes
        self._dirty_nodes = set()
        visited: Set[str] = set()

        def visit(name: str) -> None:
            if name in visited:
                return
            visited.add(name)

            # Visit all dependencies first
            for dep in self._env.iter_deps(name):
                visit(dep.source)

            # If node is not built or any dependency is dirty, mark as dirty
            if not self._env.is_node_built(name) or any(
                dep.source in self._dirty_nodes for dep in self._env.iter_deps(name)
            ):
                self._dirty_nodes.add(name)

        visit(self._target)

    async def __aiter__(self) -> AsyncIterator[str]:
        """Iterate over nodes that are ready to be built.
        
        Yields nodes in dependency order when they are ready to be built.
        A node is ready when all its dependencies are completed and it's not
        already in progress or completed.
        """
        while self._dirty_nodes:
            # Find nodes that are ready (all deps completed)
            ready_nodes = set()
            for node_name in self._dirty_nodes:
                if node_name in self._in_progress:
                    continue
                
                deps = list(self._env.iter_deps(node_name))
                if all(dep.source in self._completed for dep in deps):
                    ready_nodes.add(node_name)

            if not ready_nodes:
                # If no nodes are ready but we have dirty nodes and in-progress nodes,
                # we need to wait for in-progress nodes to complete
                if self._in_progress:
                    return
                # If no nodes are ready and nothing is in progress, we have a cycle
                raise RuntimeError("Dependency cycle detected")

            # Yield each ready node
            for node_name in ready_nodes:
                self._dirty_nodes.remove(node_name)
                self._in_progress.add(node_name)
                yield node_name
