from typing import Mapping, Sequence
from ailets.cons.atyping import Dependency, IEnvironment


class Processes:
    def __init__(self, env: IEnvironment, streams: Streams):
        self.env = env
        self.streams = streams

        # With resolved aliases
        self.deps: Mapping[str, Sequence[Dependency]] = {}
        self.rev_deps: Mapping[str, Sequence[Dependency]] = {}


    def build_deps(self):
        # Build forward dependencies
        self.deps = {}
        for node_name in self.env.get_node_names():
            self.deps[node_name] = list(self.env.iter_deps(node_name))

        # Build reverse dependencies
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
        pass
        # TODO: implement
    
    def run(self):
        pass
        # TODO: implement
        # Copy from "Environment.run()"

    def next_node_iter(self):
        pass
        # TODO: implement
        # calculate a new set:
        #  - skip finished and active nodes
        #  - add node to the set if each dep:
        #    - is built, or
        #    - there is an input in streams
        # yield each node in the set
        # wait for invalidation event

