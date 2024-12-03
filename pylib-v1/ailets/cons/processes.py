import asyncio
from typing import Mapping, Sequence
from ailets.cons.atyping import Dependency, IEnvironment, INodeRegistry
from ailets.cons.node_runtime import NodeRuntime


class Processes:
    def __init__(self, env: IEnvironment, streams: Streams):
        self.env = env
        self.streams = streams

        self.invalidation_flag = asyncio.Event()

        # With resolved aliases
        self.deps: Mapping[str, Sequence[Dependency]] = {}
        self.rev_deps: Mapping[str, Sequence[Dependency]] = {}


    def resolve_deps(self):
        self.deps = {}
        for node_name in self.env.get_node_names():
            self.deps[node_name] = list(self.env.iter_deps(node_name))

        rev_deps = {}
        for node_name, deps in self.deps.items():
            for dep in deps:
                if dep.source not in rev_deps:
                    rev_deps[dep.source] = []
                rev_deps[dep.source].append(
                    Dependency(source=node_name, name=dep.name, stream=dep.stream)
                )
        self.rev_deps = rev_deps
    
    def mark_plan_as_invalid(self):
        self.invalidation_flag.set()
    
    async def next_node_iter(self):
        while True:
            self.invalidation_flag.clear()
            for node_name in self.env.get_node_names():
                if self.env.is_node_finished(node_name) or self.env.is_node_active(node_name):
                    continue
                if self._can_start_node(node_name):
                    yield node_name
            await self.invalidation_flag.wait()

    def _can_start_node(self, node_name: str) -> bool:
        return all(
            self.env.is_node_built(dep.source) or 
            self.streams.has_input(dep.source, dep.stream)
            for dep in self.deps[node_name]
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
