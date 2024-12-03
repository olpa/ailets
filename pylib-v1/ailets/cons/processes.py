import asyncio
from typing import Mapping, Sequence
from ailets.cons.atyping import Dependency, IEnvironment


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
    
    def run(self):
        pass
        # TODO: implement
        # Copy from "Environment.run()"

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
