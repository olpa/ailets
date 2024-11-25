import dataclasses
from typing import Optional

from ailets.cons.pipelines import instantiate_with_deps

from .typing import (
    IEnvironment,
    INodeDagops,
    INodeRegistry,
    INodeRuntime,
    TypedValue,
)


class NodeDagops(INodeDagops):
    def __init__(self, env: IEnvironment, nodereg: INodeRegistry, node: INodeRuntime):
        self._env = env
        self._nodereg = nodereg
        self._node = node

    def add_typed_value_node(
        self, value: TypedValue, explain: Optional[str] = None
    ) -> str:
        node = self._env.add_typed_value_node(value, explain)
        return node.name

    def instantiate_with_deps(self, target: str, aliases: dict[str, str]) -> str:
        return instantiate_with_deps(self._env, self._nodereg, target, aliases)

    def alias(self, alias: str, node_name: Optional[str]) -> None:
        self._env.alias(alias, node_name)

    def detach_from_alias(self, alias: str) -> None:
        nodes, aliases = self._env.privates_for_dagops_friend()

        defunc_name = f"{self._env.get_next_name('defunc')}.{alias}"
        aliases[defunc_name] = list(aliases[alias])

        for node in nodes.values():
            for i, dep in enumerate(node.deps):
                if dep.source == alias:
                    node.deps[i] = dataclasses.replace(dep, source=defunc_name)
