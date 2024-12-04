import asyncio
from typing import AsyncIterator, Mapping, Sequence
from ailets.cons.atyping import Dependency, IEnvironment, IProcesses
from ailets.cons.node_runtime import NodeRuntime


class Processes(IProcesses):
    def __init__(self, env: IEnvironment):
        self.env = env
        self.streams = env.streams
        self.dagops = env.dagops

        self.deptree_invalidation_flag = asyncio.Event()

        self.finished_nodes: set[str] = set()
        self.active_nodes: set[str] = set()

        # With resolved aliases
        self.deps: Mapping[str, Sequence[Dependency]] = {}
        self.rev_deps: Mapping[str, Sequence[Dependency]] = {}

    def is_node_finished(self, name: str) -> bool:
        return name in self.finished_nodes

    def is_node_active(self, name: str) -> bool:
        return name in self.active_nodes

    def resolve_deps(self) -> None:
        self.deps = {}
        for node_name in self.dagops.get_node_names():
            self.deps[node_name] = list(self.dagops.iter_deps(node_name))

        rev_deps: dict[str, list[Dependency]] = {}
        for node_name, deps in self.deps.items():
            for dep in deps:
                if dep.source not in rev_deps:
                    rev_deps[dep.source] = []
                rev_deps[dep.source].append(
                    Dependency(source=node_name, name=dep.name, stream=dep.stream)
                )
        self.rev_deps = rev_deps

    def mark_deptree_as_invalid(self) -> None:
        self.deptree_invalidation_flag.set()

    async def next_node_iter(self) -> AsyncIterator[str]:
        while True:
            self.deptree_invalidation_flag.clear()
            for node_name in self.dagops.get_node_names():
                if node_name in self.finished_nodes or node_name in self.active_nodes:
                    continue
                if self._can_start_node(node_name):
                    yield node_name
            await self.deptree_invalidation_flag.wait()

    def _can_start_node(self, node_name: str) -> bool:
        return all(
            dep.source in self.finished_nodes or self.streams.has_input(node_name, dep)
            for dep in self.deps[node_name]
        )

    async def build_node_alone(self, name: str) -> None:
        """Build a node. Does not build its dependencies."""
        node = self.dagops.get_node(name)

        runtime = NodeRuntime(self.env, name, self.deps[name])

        # Execute the node's function with all dependencies
        try:
            self.active_nodes.add(name)
            await node.func(runtime)
            self.finished_nodes.add(name)
            self.mark_deptree_as_invalid()
        except Exception:
            print(f"Error building node '{name}'")
            print(f"Function: {node.func.__name__}")
            print("Dependencies:")
            for dep in self.deps[name]:
                print(f"  {dep.source} ({dep.stream}) -> {dep.name}")
            raise
