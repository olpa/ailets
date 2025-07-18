import dataclasses
from typing import List, Optional

from ailets.cons.flow_builder import instantiate_with_deps

from ailets.atyping import (
    IEnvironment,
    INodeDagops,
    INodeRuntime,
)


class NodeDagops(INodeDagops):
    def __init__(self, env: IEnvironment, node: INodeRuntime):
        self.env = env
        self.nodereg = env.nodereg
        self.dagops = env.dagops
        self.piper = env.piper
        self.processes = env.processes
        self.node = node
        self.handle_to_name: List[str] = ['no-node-id-0']
        self.fd_to_node_handle: dict[int, int] = {}

    def add_value_node(self, value: bytes, explain: Optional[str] = None) -> str:
        node = self.dagops.add_value_node(value, self.piper, self.processes, explain)
        return node.name

    def instantiate_with_deps(self, target: str, aliases: dict[str, str]) -> str:
        return instantiate_with_deps(self.dagops, self.nodereg, target, aliases)

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

    def v2_alias(self, alias: str, node_handle: int) -> int:
        node_name = self.handle_to_name[node_handle]

        self.alias(alias, node_name)

        self.handle_to_name.append(alias)
        return len(self.handle_to_name) - 1

    def v2_add_value_node(self, value: bytes, explain: Optional[str] = None) -> int:
        node_name = self.add_value_node(value, explain)

        self.handle_to_name.append(node_name)

        return len(self.handle_to_name) - 1

    def _resolve_alias_handle(self, alias: str, handle: int) -> str:
        """Resolve an alias using dagops if handle is 0, otherwise use handle_to_name."""
        if handle == 0:
            return alias
        else:
            return self.handle_to_name[handle]

    def v2_instantiate_with_deps(self, target: str, aliases: dict[str, int]) -> int:
        # aliases: Dict[str, int] where int can be a fd or a node handle, with fd having priority
        name_aliases = {}
        for alias, value in aliases.items():
            # Check if value is a file descriptor first (fd has priority)
            node_handle = self.find_node_by_fd(value)
            if node_handle != -1:
                # Value is a valid fd, use the corresponding node handle
                name_aliases[alias] = self._resolve_alias_handle(alias, node_handle)
            else:
                # Value is not a valid fd, treat it as a node handle
                name_aliases[alias] = self._resolve_alias_handle(alias, value)

        node_name = self.instantiate_with_deps(target, name_aliases)

        self.handle_to_name.append(node_name)
        return len(self.handle_to_name) - 1

    def open_write_pipe(self, explain: Optional[str] = None) -> int:
        node = self.dagops.add_open_value_node(
            self.piper, 
            self.processes, 
            self.env.notification_queue, 
            explain
        )
        self.handle_to_name.append(node.name)
        node_handle = len(self.handle_to_name) - 1

        try:
            pipe = self.piper.get_existing_pipe(node.name, "")
        except KeyError:
            return -1
        fd = self.node.private_store_writer(node.name, pipe)

        self.fd_to_node_handle[fd] = node_handle
        return fd

    def find_node_by_fd(self, fd: int) -> int:
        return self.fd_to_node_handle.get(fd, -1)

    def alias_fd(self, alias: str, fd: int) -> None:
        node_handle = self.find_node_by_fd(fd)
        if node_handle == -1:
            return
        node_name = self.handle_to_name[node_handle]
        self.alias(alias, node_name)
