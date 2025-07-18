from typing import Optional, Callable, Awaitable, Any
from ailets.cons.notification_queue import INotificationQueue

from ailets.atyping import (
    INodeRuntime,
    IProcesses,
    IPiper,
    Node,
)
from ailets.io.mempipe import Writer as MemPipeWriter


async def async_dummy(runtime: INodeRuntime) -> None:
    """Dummy async function for completed value nodes."""
    pass


def create_wait_for_pipe_close_func(
    full_name: str,
    writer: MemPipeWriter,
    writer_handle: int,
    notification_queue: INotificationQueue,
) -> Callable[[INodeRuntime], Awaitable[None]]:
    """Create a function that waits for pipe close via notification queue."""

    async def wait_for_pipe_close(runtime: INodeRuntime) -> None:
        """Wait until the pipe is closed via notification queue"""
        if writer_handle == -1:
            return  # Skip waiting if we can't get the handle

        queue = notification_queue
        lock = queue.get_lock()

        # Loop until writer is closed
        while not writer.closed:
            with lock:
                if not writer.closed:
                    await queue.wait_unsafe(writer_handle, f"value node {full_name}")

    return wait_for_pipe_close


def _create_value_node_base(
    dagops: Any,
    piper: IPiper,
    node_func: Callable[[INodeRuntime], Awaitable[None]],
    explain: Optional[str] = None,
) -> tuple[Node, MemPipeWriter]:
    """Shared logic for creating value nodes.

    Returns:
        Tuple of (node, writer) for further customization
    """
    full_name = dagops.get_next_name("value")

    node = Node(
        name=full_name,
        func=node_func,
        deps=[],  # No dependencies
        explain=explain,
    )

    dagops.nodes[full_name] = node

    # Create the pipe
    pipe = piper.create_pipe(full_name, "")
    writer = pipe.get_writer()
    assert isinstance(
        writer, MemPipeWriter
    ), "Internal error: MemPipeWriter is expected"

    return node, writer


def add_value_node(
    dagops: Any,
    value: bytes,
    piper: IPiper,
    processes: IProcesses,
    explain: Optional[str] = None,
) -> Node:
    """Add a typed value node to the environment.

    Args:
        dagops: Dagops instance for getting next name and storing node
        value: The value to store
        piper: Piper for creating pipes
        processes: Process manager for marking node as finished
        explain: Optional explanation of what the value represents

    Returns:
        The created node
    """
    node, writer = _create_value_node_base(dagops, piper, async_dummy, explain)

    # Set the value in the pipe and close it
    writer.write_sync(value)
    writer.close()
    processes.add_finished_node(node.name)

    return node


def add_open_value_node(
    dagops: Any,
    piper: IPiper,
    notification_queue: INotificationQueue,
    explain: Optional[str] = None,
) -> Node:
    """Create a value node without closing the pipe or marking as completed.

    Args:
        dagops: Dagops instance for getting next name and storing node
        piper: Piper for creating the pipe
        notification_queue: Queue to listen for pipe close events
        explain: Optional explanation of what the value represents

    Returns:
        The created node with an open pipe for writing
    """
    # Create node with a temporary function, we'll update it after getting the writer
    node, writer = _create_value_node_base(dagops, piper, async_dummy, explain)

    # Get writer handle and create the proper wait function
    writer_handle = writer.handle if isinstance(writer, MemPipeWriter) else -1
    wait_for_pipe_close = create_wait_for_pipe_close_func(
        node.name, writer, writer_handle, notification_queue
    )

    # Update the node with the correct function
    updated_node = Node(
        name=node.name,
        func=wait_for_pipe_close,
        deps=node.deps,
        explain=node.explain,
    )
    dagops.nodes[node.name] = updated_node

    return updated_node
