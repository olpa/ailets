import asyncio
import itertools
import logging
import sys
from typing import Iterator, Mapping, Optional, Sequence
from ailets.cons.atyping import Dependency, IEnvironment, IProcesses
from ailets.cons.node_runtime import NodeRuntime


logger = logging.getLogger("ailets.processes")


class Processes(IProcesses):
    def __init__(self, env: IEnvironment):
        self.env = env
        self.streams = env.streams
        self.dagops = env.dagops
        self.queue = env.notification_queue

        self.finished_nodes: set[str] = set()
        self.active_nodes: set[str] = set()

        # With resolved aliases
        self.deps: Mapping[str, Sequence[Dependency]] = {}
        self.rev_deps: Mapping[str, Sequence[Dependency]] = {}

        self.progress_handle: int = env.seqno.next_seqno()
        self.queue.whitelist(self.progress_handle, "ailets.processes")
        self.pool: set[asyncio.Task[None]] = set()
        self.loop = asyncio.get_event_loop()

    def destroy(self) -> None:
        self.queue.unlist(self.progress_handle)

    def is_node_finished(self, name: str) -> bool:
        return name in self.finished_nodes

    def is_node_active(self, name: str) -> bool:
        return name in self.active_nodes

    def add_value_node(self, name: str) -> None:
        self.finished_nodes.add(name)

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

    def get_nodes_to_build(self, target_node_name: str) -> list[str]:
        nodes_to_build = []
        visited = set()

        def visit_node(node_name: str) -> None:
            if node_name in visited:
                return
            visited.add(node_name)

            # Visit dependencies first
            if node_name in self.deps:
                for dep in self.deps[node_name]:
                    visit_node(dep.source)

            # Add this node if not already built
            if node_name not in self.finished_nodes:
                nodes_to_build.append(node_name)

        visit_node(target_node_name)
        return nodes_to_build

    # Infinite iterator that yields nodes to build as they are ready to be built
    # Returns None if no nodes are ready to be built
    def next_node_iter(
        self,
        target_node_name: str,
        flag_one_step: bool,
        stop_before: Optional[str],
        stop_after: Optional[str],
    ) -> Iterator[str | None]:
        is_finished = False
        yielded_nodes: set[str] = set()

        # Outer loop: deptree is invalidated
        while not is_finished:

            nodes_to_build = self.get_nodes_to_build(target_node_name)

            last_hash = self.dagops.hash_of_nodenames()

            # Inner loop: return nodes to build as they are ready to be built
            has_yielded = False
            for node_name in nodes_to_build:
                if is_finished:
                    break
                if node_name in yielded_nodes:
                    continue
                if last_hash != self.dagops.hash_of_nodenames():
                    break
                if node_name in self.finished_nodes or node_name in self.active_nodes:
                    continue
                if self._can_start_node(node_name):
                    if (
                        flag_one_step
                        or node_name == stop_before
                        or node_name == stop_after
                    ):
                        is_finished = True
                    if node_name != stop_before:
                        yielded_nodes.add(node_name)
                        yield node_name

            if is_finished:
                break
            if not has_yielded:
                yield None

            if last_hash != self.dagops.hash_of_nodenames():
                logger.debug("Node set is changed in next_node_iter")
                self.resolve_deps()

        while True:
            yield None

    def _can_start_node(self, node_name: str) -> bool:
        return all(
            dep.source in self.finished_nodes or self.streams.has_input(dep)
            for dep in self.deps[node_name]
        )

    async def run_nodes(self, node_iter: Iterator[str | None]) -> None:
        self.pool = set()

        async def awaker() -> None:
            lock = self.queue.get_lock()
            lock.acquire()
            await self.queue.wait_for_handle_unsafe(
                self.progress_handle, "process.awaker"
            )

        def extend_pool() -> None:
            node_names: Sequence[str] = list(
                name
                for name in itertools.takewhile(lambda x: x is not None, node_iter)
                if isinstance(name, str)
            )
            self.pool.update(
                asyncio.create_task(self.build_node_alone(name), name=name)
                for name in node_names
            )

        extend_pool()
        awaker_task: asyncio.Task[None] = asyncio.create_task(
            awaker(), name="process.awaker"
        )
        self.pool.add(awaker_task)

        while len(self.pool) > 1:  # The awaker is always in the pool
            if awaker_task.done():
                self.pool.remove(awaker_task)
                awaker_task = asyncio.create_task(awaker(), name="process.awaker")
                self.pool.add(awaker_task)

            done, self.pool = await asyncio.wait(
                self.pool, return_when=asyncio.FIRST_COMPLETED
            )
            for task in done:
                if exc := task.exception():
                    raise exc

            extend_pool()

        awaker_task.cancel()
        self.pool.remove(awaker_task)

    async def build_node_alone(self, name: str) -> None:
        """Build a node. Does not build its dependencies."""
        logger.debug(f"Starting to build node '{name}'")
        node = self.dagops.get_node(name)

        runtime = NodeRuntime(self.env, name, self.deps[name])

        # Execute the node's function with all dependencies
        try:
            self.active_nodes.add(name)
            await node.func(runtime)
            self.finished_nodes.add(name)
        except Exception:
            exc = sys.exc_info()[1]
            print(f"Error building node '{name}'")
            print(f"Function: {node.func.__name__}")
            print("Dependencies:")
            for dep in self.deps[name]:
                print(f"  {dep.source} ({dep.stream}) -> {dep.name}")
            print(f"Exception: {exc}")
            raise
        finally:
            self.queue.notify(self.progress_handle)

    def get_processes(self) -> set[asyncio.Task[None]]:
        return self.pool

    def get_progress_handle(self) -> int:
        return self.progress_handle
