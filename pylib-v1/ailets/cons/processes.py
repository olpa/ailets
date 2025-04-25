import asyncio
import itertools
import logging
import sys
import threading
from typing import Iterator, Mapping, Optional, Sequence
from ailets.atyping import (
    Dependency,
    IEnvironment,
    IProcesses,
)
from ailets.actor_runtime.node_runtime import NodeRuntime


logger = logging.getLogger("ailets.processes")


class Processes(IProcesses):
    def __init__(self, env: IEnvironment):
        self.env = env
        self.piper = env.piper
        self.dagops = env.dagops
        self.queue = env.notification_queue

        self.finished_nodes: set[str] = set()
        self.active_nodes: set[str] = set()
        self.completion_codes: dict[str, int] = {}
        # With resolved aliases
        self.deps: Mapping[str, Sequence[Dependency]] = {}
        self.rev_deps: Mapping[str, Sequence[Dependency]] = {}

        self.progress_handle: int = env.seqno.next_seqno()
        self.queue.whitelist(self.progress_handle, "ailets.processes")
        self.progress_seq = 0
        self.progress_lock = threading.Lock()
        logger.debug("Processes: progress_handle is: %s", self.progress_handle)

        self.pool: set[asyncio.Task[None]] = set()
        self.loop = asyncio.get_event_loop()

        self.fsops_subscription_id: int | None = None
        self.fsops_handle: int | None = None
        self.subscribe_fsops()

    def destroy(self) -> None:
        self.unsubscribe_fsops()
        self.queue.unlist(self.progress_handle)

    def subscribe_fsops(self) -> None:
        self.fsops_handle = self.piper.get_fsops_handle()

        def on_fsops(writer_handle: int) -> None:
            async def awake_on_write() -> None:
                lock = self.queue.get_lock()
                lock.acquire()
                await self.queue.wait_unsafe(writer_handle, "process.awaker_on_write")
                self.notify_progress(writer_handle)

            asyncio.create_task(awake_on_write(), name="process.awaker_on_write")
            self.notify_progress(writer_handle)

        self.fsops_subscription_id = self.queue.subscribe(
            self.fsops_handle, on_fsops, "Processes: observe fsops"
        )

    def unsubscribe_fsops(self) -> None:
        if self.fsops_subscription_id is not None:
            if self.fsops_handle is not None:
                self.queue.unsubscribe(self.fsops_handle, self.fsops_subscription_id)
            self.fsops_handle = None
            self.fsops_subscription_id = None

    def notify_progress(self, hint_handle: int) -> None:
        with self.progress_lock:
            self.progress_seq += 1
        self.queue.notify(self.progress_handle, hint_handle)

    def is_node_finished(self, name: str) -> bool:
        return name in self.finished_nodes

    def is_node_active(self, name: str) -> bool:
        return name in self.active_nodes

    def add_finished_node(self, name: str) -> None:
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
                    Dependency(source=node_name, name=dep.name, slot=dep.slot)
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
        def dep_is_progressed(dep: Dependency) -> bool:
            if dep.source in self.finished_nodes:
                return True
            if dep.source not in self.active_nodes:
                return False
            # Look at the pipes only after checking that the dependency is stated
            # Otherwise, a pipe is opened in the read mode before the pipe is created,
            # and the code will raise an error on pipe creation later.
            try:
                pipe = self.piper.get_existing_pipe(dep.source, dep.slot)
            except KeyError:
                return False
            return pipe.get_writer().tell() > 0

        return all(dep_is_progressed(dep) for dep in self.deps[node_name])

    async def run_nodes(self, node_iter: Iterator[str | None]) -> None:
        self.pool = set()

        async def awaker() -> None:
            lock = self.queue.get_lock()
            lock.acquire()
            await self.queue.wait_unsafe(self.progress_handle, "process.awaker")

        def extend_pool() -> None:
            if self.env.get_errno() != 0:
                return
            node_names: Sequence[str] = list(
                name
                for name in itertools.takewhile(lambda x: x is not None, node_iter)
                if isinstance(name, str)
            )
            self.pool.update(
                asyncio.create_task(self.build_node_alone(name), name=name)
                for name in node_names
            )

        awaker_task: Optional[asyncio.Task[None]] = None

        def refresh_awaker_in_pool() -> None:
            nonlocal awaker_task
            if awaker_task is not None and awaker_task.done():
                if awaker_task in self.pool:
                    self.pool.remove(awaker_task)
                awaker_task = None
            if len(self.pool) > 0:
                if awaker_task is None:
                    awaker_task = asyncio.create_task(awaker(), name="process.awaker")
                self.pool.add(awaker_task)

        extend_pool()

        while len(self.pool) > 0:
            refresh_awaker_in_pool()
            if len(self.pool) == 1:  # only awaker, no real tasks
                break

            done, self.pool = await asyncio.wait(
                self.pool, return_when=asyncio.FIRST_COMPLETED
            )

            # Should never happen: the actor wrapper should catch all exceptions
            for task in done:
                if exc := task.exception():
                    raise exc

            # In case of an error:
            # Don't start new actors (the condition is inside `extend_pool`)
            # For currently running nodes, expect that they eventually will get
            # "pipe broken" error and will finish on their own.
            extend_pool()

        # Awake awaker
        if awaker_task is not None:
            if not awaker_task.done():
                self.notify_progress(-1)
                await awaker_task

    async def build_node_alone(self, name: str) -> None:
        """Build a node. Does not build its dependencies."""
        logger.debug(f"Starting to build node '{name}'")
        node = self.dagops.get_node(name)

        node_runtime = NodeRuntime(self.env, name, self.deps[name])

        # Execute the node's function with all dependencies
        try:
            self.active_nodes.add(name)
            await node.func(node_runtime)
        except Exception as exc:
            print(f"*** ailet error: {name}: {str(exc)}", file=sys.stderr)
            if node_runtime.get_errno() == 0:
                node_runtime.set_errno(-1)  # the rest in `finally`
            if logger.isEnabledFor(logging.DEBUG):
                logger.debug(f"Error building node '{name}'")
                logger.debug(f"Function: {node.func.__name__}")
                logger.debug("Dependencies:")
                for dep in self.deps[name]:
                    logger.debug(f"  {dep.source} ({dep.slot}) -> {dep.name}")
                logger.debug(f"Exception: {exc}")
                logger.debug("Stack trace:", exc_info=True)
        finally:
            ccode = node_runtime.get_errno()
            logger.debug(
                f"Finished building node '{name}' with code {ccode}, making accounting"
            )
            self.set_completion_code(name, ccode)
            self.finished_nodes.add(name)
            await node_runtime.destroy()
            self.notify_progress(-1)

    def get_processes(self) -> set[asyncio.Task[None]]:
        return self.pool

    def get_progress_handle(self) -> int:
        return self.progress_handle

    def set_completion_code(self, name: str, ccode: int) -> None:
        self.completion_codes[name] = ccode
        if ccode != 0 and self.env.get_errno() == 0:
            self.env.set_errno(ccode)

    def get_optional_completion_code(self, name: str) -> Optional[int]:
        return self.completion_codes.get(name)
