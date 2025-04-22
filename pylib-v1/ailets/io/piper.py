import io
import json
from typing import IO, Any, Dict, Literal, Optional

from ailets.atyping import (
    IAsyncReader,
    IAsyncWriter,
    IKVBuffers,
    IPipe,
    IPiper,
)
from ailets.cons.util import get_path
from ailets.io.mempipe import (
    MemPipe,
    Writer as MemPipeWriter,
    Reader as MemPipeReader,
)
from ailets.cons.notification_queue import DummyNotificationQueue, INotificationQueue
from ailets.cons.seqno import Seqno

import logging

logger = logging.getLogger("ailets.piper")


class PrintWrapper(IPipe):
    class Writer(IAsyncWriter):
        def __init__(self, output: IO[str], writer: Optional[IAsyncWriter]) -> None:
            self.output = output
            self.writer = writer
            self.closed = False
            self.errno = 0

        async def write(self, data: bytes) -> int:
            self.output.write(data.decode("utf-8"))
            self.output.flush()
            if self.writer is None:
                return len(data)

            n = await self.writer.write(data)
            self.closed = self.writer.closed
            return n

        def tell(self) -> int:
            if self.writer is not None:
                return self.writer.tell()
            return self.output.tell()

        def close(self) -> None:
            if self.writer is not None:
                self.writer.close()
            self.closed = True

        def get_error(self) -> int:
            if self.writer is not None:
                return self.writer.get_error()
            return self.errno

        def set_error(self, errno: int) -> None:
            self.errno = errno
            if self.writer is not None:
                self.writer.set_error(errno)

        def __str__(self) -> str:
            return (
                f"PrintWrapper.Writer(output={self.output}, "
                f"closed={self.closed}, writer={self.writer}, "
                f"errno={self.errno})"
            )

    def __init__(self, output: IO[str], pipe: Optional[IPipe]) -> None:
        self.pipe = pipe
        wrapped_writer = pipe.get_writer() if pipe else None
        self.writer = PrintWrapper.Writer(output, wrapped_writer)

    def get_writer(self) -> IAsyncWriter:
        return self.writer

    def get_reader(self, handle: int) -> IAsyncReader:
        if self.pipe is None:
            raise io.UnsupportedOperation("PrintWrapper is write-only")
        return self.pipe.get_reader(handle)

    def __str__(self) -> str:
        return f"PrintWrapper(pipe={self.pipe}, " f"writer={self.writer})"


class StaticInput(IPipe):
    def __init__(self, content: bytes, debug_hint: str) -> None:
        writer = MemPipeWriter(
            handle=-1,
            queue=DummyNotificationQueue(),
            debug_hint=debug_hint,
        )
        writer.write_sync(content)
        writer.close()
        self.writer = writer

    def get_reader(self, handle: int) -> IAsyncReader:
        return MemPipeReader(handle, writer=self.writer)

    def get_writer(self) -> IAsyncWriter:
        raise io.UnsupportedOperation("StaticInput is read-only")


class Piper(IPiper):
    """Manages pipes for an environment."""

    def __init__(
        self,
        kv: IKVBuffers,
        notification_queue: INotificationQueue,
        seqno: Seqno,
    ) -> None:
        self.kv = kv
        self.seqno = seqno
        self.queue = notification_queue
        self.fsops_handle = -1
        self.init_fsops_handle()
        self.pipes: Dict[str, IPipe] = {}

    def destroy(self) -> None:
        self.destroy_fsops_handle()

    def init_fsops_handle(self) -> None:
        self.fsops_handle = self.seqno.next_seqno()
        self.queue.whitelist(self.fsops_handle, "Piper: file system operations")
        logger.debug("Piper: fsops_handle is: %s", self.fsops_handle)

    def get_fsops_handle(self) -> int:
        return self.fsops_handle

    def destroy_fsops_handle(self) -> None:
        self.queue.unlist(self.fsops_handle)
        self.fsops_handle = -1

    def create_pipe(
        self,
        node_name: str,
        slot_name: Optional[str],
        open_mode: Literal["read", "write", "append"] = "write",
    ) -> IPipe:
        """Add a new slot.
        For "write" and "append", raise KeyError if the slot already exists.
        For "read", return the existing pipe if it exists.
        If not, it tries to open from the kv.
        """
        path = get_path(node_name, slot_name)
        pipe = self.pipes.get(path, None)
        if pipe is not None:
            if open_mode == "read":
                return pipe
            raise KeyError(f"Path already exists: {path}")

        # In case of loading data from a state dump file,
        # use "append" mode to avoid overwriting the existing data.
        # In case node_runtime wants to read data, use "read" mode.
        kvbuf = self.kv.open(path, open_mode)

        writer_handle = self.seqno.next_seqno()
        pipe = MemPipe(
            writer_handle=writer_handle,
            queue=self.queue,
            debug_hint=path,
            external_buffer=kvbuf.borrow_mut_buffer(),
        )

        if open_mode == "read":
            # There was no pipe, therefore there is no writer.
            # Close the newly created unused writer to mark
            # the created pipe as complete.
            pipe.get_writer().close()

        self.pipes[path] = pipe
        logger.debug(f"Created pipe: {pipe}")

        self.queue.notify(self.fsops_handle, writer_handle)
        return pipe

    @staticmethod
    def make_env_pipe(params: Dict[str, Any]) -> IPipe:
        content = json.dumps(params).encode("utf-8")
        pipe = StaticInput(content, "env")
        return pipe

    def get_existing_pipe(self, node_name: str, slot_name: str) -> IPipe:
        """If the pipe does not exist, and there is no kv entry, raise KeyError.
        Otherwise, create it as a read-only pipe."""
        return self.create_pipe(node_name, slot_name, "read")
