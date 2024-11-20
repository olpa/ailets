import dataclasses
from typing import Iterator, Optional, Sequence, Dict, Set

from ailets.cons.pipelines import instantiate_with_deps

from .util import to_basename
from .typing import (
    Dependency,
    IEnvironment,
    INodeDagops,
    INodeRegistry,
    INodeRuntime,
)


class NodeDagops(INodeDagops):
    def __init__(self, env: IEnvironment, nodereg: INodeRegistry, node: INodeRuntime):
        self._env = env
        self._nodereg = nodereg
        self._node = node

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

    def instantiate_with_deps(self, target: str, aliases: dict[str, str]) -> str:
        return instantiate_with_deps(self._env, self._nodereg, target, aliases)

    def alias(self, alias: str, node_name: Optional[str]) -> None:
        self._env.alias(alias, node_name)

    def expand_alias(self, alias: str) -> Sequence[str]:
        return self._env.expand_alias(alias)

    def defunc_downstream(self, upstream_node_name: str, fence: Set[str]) -> None:
        nodes, aliases = self._env.privates_for_dagops_friend()

        def iter_expand_to_node_names(name: str, seen: Set[str]) -> Iterator[str]:
            if name in seen:
                return
            seen.add(name)

            if name in aliases:
                for aliased_name in aliases[name]:
                    yield from iter_expand_to_node_names(aliased_name, seen)
            elif name in nodes:
                yield name
            else:
                raise ValueError(f"Unknown name: {name}")

        #
        # Build reverse dependency map
        #
        nodedeps_reverse: Dict[str, Set[str]] = {}

        for node in nodes.values():
            for dep in node.deps:
                for dep_name in iter_expand_to_node_names(dep.source, set()):
                    if dep_name not in nodedeps_reverse:
                        nodedeps_reverse[dep_name] = set()
                    nodedeps_reverse[dep_name].add(node.name)

        #
        # Collect affected node
        #
        affected_nodes: Set[str] = set()

        node_queue: Set[str] = set()
        for upstream_node_name in iter_expand_to_node_names(upstream_node_name, set()):
            node_queue.update(nodedeps_reverse.get(upstream_node_name, set()))

        fence_nodes: Set[str] = set()
        for name in fence:
            fence_nodes.update(iter_expand_to_node_names(name, set()))

        while node_queue:
            name = node_queue.pop()
            if name.startswith("defunc."):
                continue
            if name in fence_nodes:
                continue
            if not self._env.is_node_ever_started(name):
                continue
            affected_nodes.add(name)
            for next_name in nodedeps_reverse.get(name, set()):
                if next_name not in node_queue and next_name not in affected_nodes:
                    node_queue.add(next_name)

        #
        # Rename affected nodes to "defunc.<name>"
        #
        rewrite_map: Dict[str, str] = {}
        for name in affected_nodes:
            defunc_name = f"defunc.{name}"
            rewrite_map[name] = defunc_name
            nodes[defunc_name] = dataclasses.replace(nodes[name], name=defunc_name)
            del nodes[name]

        #
        # Rewrite all dependencies to defunc nodes
        #
        for node in nodes.values():
            for i, dep in enumerate(node.deps):
                if dep.source in rewrite_map:
                    node.deps[i] = dataclasses.replace(
                        dep, source=rewrite_map[dep.source]
                    )
        for alias_list in aliases.values():
            for i, name in enumerate(alias_list):
                if name in rewrite_map:
                    alias_list[i] = rewrite_map[name]

    def detach_from_alias(self, alias: str) -> None:
        nodes, aliases = self._env.privates_for_dagops_friend()

        defunc_name = f"{self._env.get_next_name('defunc')}.{alias}"
        aliases[defunc_name] = list(aliases[alias])

        for node in nodes.values():
            for i, dep in enumerate(node.deps):
                if dep.source == alias:
                    node.deps[i] = dataclasses.replace(dep, source=defunc_name)
