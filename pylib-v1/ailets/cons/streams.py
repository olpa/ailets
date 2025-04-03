import io
import json
import sys
from typing import IO, Any, Dict, Optional, Sequence

from ailets.cons.atyping import (
    Dependency,
    IAsyncReader,
    IAsyncWriter,
    IKVBuffers,
    IStreams,
    IPipe,
)
from ailets.cons.bytesrw import (
    BytesWR,
    Writer as BytesWRWriter,
    Reader as BytesWRReader,
)
from ailets.cons.notification_queue import DummyNotificationQueue, INotificationQueue
from ailets.cons.seqno import Seqno

import logging

logger = logging.getLogger("ailets.streams")


class PrintStream(IPipe):
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
            return f"PrintStream.Writer(output={self.output}, closed={self.closed})"

    def __init__(self, output: IO[str]) -> None:
        self.writer = PrintStream.Writer(output)

    def get_writer(self) -> IAsyncWriter:
        return self.writer

    def get_reader(self, _handle: int) -> IAsyncReader:
        raise io.UnsupportedOperation("PrintStream is write-only")


class StaticStream(IPipe):
    def __init__(self, content: bytes, debug_hint: str) -> None:
        writer = BytesWRWriter(
            handle=-1,
            queue=DummyNotificationQueue(),
            debug_hint=debug_hint,
        )
        writer.write_sync(content)
        writer.close()
        self.writer = writer

    def get_reader(self, handle: int) -> IAsyncReader:
        return BytesWRReader(handle, writer=self.writer)

    def get_writer(self) -> IAsyncWriter:
        raise io.UnsupportedOperation("StaticStream is read-only")


def create_log_stream() -> IPipe:
    pipe = PrintStream(sys.stdout)
    return pipe


class Streams(IStreams):
    """Manages streams for an environment."""

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
        self.queue.whitelist(self.fsops_handle, "Streams: file system operations")

    def get_fsops_handle(self) -> int:
        return self.fsops_handle

    def destroy_fsops_handle(self) -> None:
        self.queue.unlist(self.fsops_handle)
        self.fsops_handle = -1

    def get_path(self, node_name: str, stream_name: Optional[str]) -> str:
        if not stream_name:
            return node_name
        return f"{node_name}-{stream_name}"

    def create(
        self,
        node_name: str,
        stream_name: Optional[str],
    ) -> IPipe:
        """Add a new stream."""
        if stream_name == "log":
            return create_log_stream()

        path = self.get_path(node_name, stream_name)
        kvbuf = self.kv.open(path, "write")

        writer_handle = self.seqno.next_seqno()
        pipe = BytesWR(
            writer_handle=writer_handle,
            queue=self.queue,
            debug_hint=path,
            external_buffer=kvbuf.borrow_mut_buffer(),
        )

        self.pipes[path] = pipe
        logger.debug(f"Created pipe: {pipe}")

        self.queue.notify(self.fsops_handle, writer_handle)
        return pipe

    async def mark_finished(self, node_name: str, stream_name: Optional[str]) -> None:
        path = self.get_path(node_name, stream_name)
        pipe = self.pipes[path]
        pipe.get_writer().close()

    @staticmethod
    def make_env_stream(params: Dict[str, Any]) -> IPipe:
        content = json.dumps(params).encode("utf-8")
        pipe = StaticStream(content, "env")
        return pipe

    def collect_streams(
        self,
        deps: Sequence[Dependency],
    ) -> Sequence[IPipe]:
        collected: list[IPipe] = []
        for dep in deps:
            dep_path = self.get_path(dep.source, dep.stream)
            if dep_path in self.pipes:
                collected.append(self.pipes[dep_path])
        return collected

    def has_input(self, dep: Dependency) -> bool:
        path = self.get_path(dep.source, dep.stream)
        if path not in self.pipes:
            return False
        pipe = self.pipes[path]
        return pipe.get_writer().tell() > 0
