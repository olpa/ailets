import io
import json
import sys
from typing import IO, Any, Dict, Optional, Sequence

from ailets.cons.atyping import (
    Dependency,
    IAsyncReader,
    IAsyncWriter,
    IStreams,
    IPipe,
    Stream,
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
            return len(data)

        def tell(self) -> int:
            return self.output.tell()

        def close(self) -> None:
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
    def __init__(self, content: bytes) -> None:
        writer = BytesWRWriter(handle=-1, queue=DummyNotificationQueue())
        writer.write_sync(content)
        writer.close()
        self.writer = writer

    def get_reader(self, handle: int) -> IAsyncReader:
        return BytesWRReader(handle, writer=self.writer)

    def get_writer(self) -> IAsyncWriter:
        raise io.UnsupportedOperation("StaticStream is read-only")


def create_log_stream() -> Stream:
    pipe = PrintStream(sys.stdout)
    return Stream(
        node_name=".",
        stream_name="log",
        pipe=pipe,
    )


class Streams(IStreams):
    """Manages streams for an environment."""

    def __init__(self, notification_queue: INotificationQueue, seqno: Seqno) -> None:
        self._streams: list[Stream] = []
        self.seqno = seqno
        self.queue = notification_queue

    def _find_stream(
        self, node_name: str, stream_name: Optional[str]
    ) -> Optional[Stream]:
        return next(
            (
                s
                for s in self._streams
                if s.node_name == node_name and s.stream_name == stream_name
            ),
            None,
        )

    def get(self, node_name: str, stream_name: Optional[str]) -> Stream:
        stream = self._find_stream(node_name, stream_name)
        if stream is None:
            raise ValueError(f"Stream not found: {node_name}.{stream_name}")
        return stream

    def create(
        self,
        node_name: str,
        stream_name: Optional[str],
        initial_content: Optional[bytes] = None,
        is_closed: bool = False,
    ) -> Stream:
        """Add a new stream."""
        if stream_name == "log":
            log_pipe = PrintStream(sys.stdout)
            return Stream(
                node_name=node_name,
                stream_name=stream_name,
                pipe=log_pipe,
            )

        if self._find_stream(node_name, stream_name) is not None:
            raise ValueError(f"Stream already exists: {node_name}.{stream_name}")

        pipe = BytesWR(
            writer_handle=self.seqno.next_seqno(),
            queue=self.queue,
        )
        writer = pipe.get_writer()
        if isinstance(writer, BytesWRWriter):
            if initial_content is not None:
                writer.write_sync(initial_content)
            if is_closed:
                writer.close()

        stream = Stream(
            node_name=node_name,
            stream_name=stream_name,
            pipe=pipe,
        )
        logger.debug(f"Created stream: {stream}")

        self._streams.append(stream)
        return stream

    async def mark_finished(self, node_name: str, stream_name: Optional[str]) -> None:
        """Mark a stream as finished."""
        stream = self.get(node_name, stream_name)
        stream.pipe.get_writer().close()

    @staticmethod
    def make_env_stream(params: Dict[str, Any]) -> Stream:
        content = json.dumps(params).encode("utf-8")
        pipe = StaticStream(content)
        return Stream(
            node_name=".",
            stream_name="env",
            pipe=pipe,
        )

    def get_fs_output_streams(self) -> Sequence[Stream]:
        return [
            s
            for s in self._streams
            if s.stream_name is not None and s.stream_name.startswith("out/")
        ]

    def collect_streams(
        self,
        deps: Sequence[Dependency],
    ) -> Sequence[Stream]:
        collected: list[Stream] = []
        for dep in deps:
            collected.extend(
                s
                for s in self._streams
                if s.node_name == dep.source and s.stream_name == dep.stream
            )
        return collected

    async def read_dir(self, dir_name: str, node_names: Sequence[str]) -> Sequence[str]:
        if not dir_name.endswith("/"):
            dir_name = f"{dir_name}/"
        pos = len(dir_name)
        return [
            s.stream_name[pos:]
            for s in self._streams
            if s.node_name in node_names
            and s.stream_name is not None
            and s.stream_name.startswith(dir_name)
        ]

    def has_input(self, dep: Dependency) -> bool:
        stream = next(
            (
                s
                for s in self._streams
                if s.node_name == dep.source and s.stream_name == dep.stream
            ),
            None,
        )
        return stream is not None and stream.pipe.get_writer().tell() > 0
