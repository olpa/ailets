import io
import json
import sys
from typing import IO, Any, Dict, Literal, Optional

from ailets.cons.atyping import (
    IAsyncReader,
    IAsyncWriter,
    IKVBuffers,
    IPipe,
    IPiper,
)
from ailets.cons.mempipe import (
    MemPipe,
    Writer as MemPipeWriter,
    Reader as MemPipeReader,
)
from ailets.cons.notification_queue import DummyNotificationQueue, INotificationQueue
from ailets.cons.seqno import Seqno

import logging

logger = logging.getLogger("ailets.piper")


class PrintOutput(IPipe):
    class Writer(IAsyncWriter):
        def __init__(self, output: IO[str]) -> None:
            self.output = output
            self.closed = False

        async def write(self, data: bytes) -> int:
            self.output.write(data.decode("utf-8"))
            self.output.flush()
            return len(data)

        def tell(self) -> int:
            return self.output.tell()

        def close(self) -> None:
            if not self.output == sys.stdout:
                self.output.close()
                self.closed = True

        def __str__(self) -> str:
            return f"PrintOutput.Writer(output={self.output}, closed={self.closed})"

    def __init__(self, output: IO[str]) -> None:
        self.writer = PrintOutput.Writer(output)

    def get_writer(self) -> IAsyncWriter:
        return self.writer

    def get_reader(self, _handle: int) -> IAsyncReader:
        raise io.UnsupportedOperation("PrintOutput is write-only")


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

    def get_fsops_handle(self) -> int:
        return self.fsops_handle

    def destroy_fsops_handle(self) -> None:
        self.queue.unlist(self.fsops_handle)
        self.fsops_handle = -1

    def get_path(self, node_name: str, slot_name: Optional[str]) -> str:
        if not slot_name:
            return node_name
        if "/" in slot_name:
            return slot_name
        return f"{node_name}-{slot_name}"

    def create_pipe(
        self,
        node_name: str,
        slot_name: Optional[str],
        open_mode: Literal["read", "write", "append"] = "write",
    ) -> IPipe:
        """Add a new slot. Raise KeyError if the slot already exists."""
        path = self.get_path(node_name, slot_name)
        if path in self.pipes:
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

        self.pipes[path] = pipe
        logger.debug(f"Created pipe: {pipe}")

        self.queue.notify(self.fsops_handle, writer_handle)
        return pipe

    @staticmethod
    def make_env_pipe(params: Dict[str, Any]) -> IPipe:
        content = json.dumps(params).encode("utf-8")
        pipe = StaticInput(content, "env")
        return pipe

    @staticmethod
    def make_log_pipe() -> IPipe:
        pipe = PrintOutput(sys.stdout)
        return pipe

    def get_existing_pipe(self, node_name: str, slot_name: str) -> IPipe:
        path = self.get_path(node_name, slot_name)
        if path not in self.pipes:
            raise KeyError(f"Path not found: {path}")
        return self.pipes[path]
