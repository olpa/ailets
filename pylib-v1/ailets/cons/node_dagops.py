import dataclasses
from typing import Optional

from ailets.cons.pipelines import instantiate_with_deps

from .atyping import (
    IEnvironment,
    INodeDagops,
    INodeRuntime,
)


class NodeDagops(INodeDagops):
    def __init__(self, env: IEnvironment, node: INodeRuntime):
        self.nodereg = env.nodereg
        self.dagops = env.dagops
        self.node = node

    def add_value_node(self, value: bytes, explain: Optional[str] = None) -> str:
        node = self.dagops.add_value_node(value, explain)
        return node.name

    def instantiate_with_deps(self, target: str, aliases: dict[str, str]) -> str:
        return instantiate_with_deps(
            self.dagops, self.nodereg, target, aliases
        )

    def alias(self, alias: str, node_name: Optional[str]) -> None:
        self.dagops.alias(alias, node_name)

    def detach_from_alias(self, alias: str) -> None:
        nodes, aliases = self.dagops.privates_for_dagops_friend()

        defunc_name = f"{self.dagops.get_next_name('defunc')}.{alias}"
        aliases[defunc_name] = list(aliases[alias])

        for node in nodes.values():
            for i, dep in enumerate(node.deps):
                if dep.source == alias:
                    node.deps[i] = dataclasses.replace(dep, source=defunc_name)
