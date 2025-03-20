from dataclasses import dataclass
import io
import json
from typing import Any, Callable, Dict, Optional, Sequence

from ailets.cons.atyping import Dependency, IStreams, INotificationQueue, Stream
from ailets.cons.bytesrw import BytesWR


def create_log_stream() -> Stream:
    class LogStream(AsyncBuffer):
        async def write(self, b: bytes) -> int:
            b2 = b.decode("utf-8")
            print(b2, end="")
            return len(b2)

    return Stream(
        node_name=".",
        stream_name="log",
        buf=LogStream(b"", False, lambda: None),
    )


class Streams(IStreams):
    """Manages streams for an environment."""

    def __init__(self, notification_queue: INotificationQueue, id_generator: Callable[[], int]) -> None:
        self._streams: list[Stream] = []
        self.on_write_started: Callable[[], None] = lambda: None
        self.idgen = id_generator
        self.queue = notification_queue

    def set_on_write_started(self, on_write_started: Callable[[], None]) -> None:
        self.on_write_started = on_write_started

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
            return create_log_stream()

        if self._find_stream(node_name, stream_name) is not None:
            raise ValueError(f"Stream already exists: {node_name}.{stream_name}")

        pipe = BytesWR(
            writer_handle=self.idgen(),
            queue=self.queue,
        )
        if initial_content is not None:
            pipe.write(initial_content)
        if is_closed:
            pipe.close()

        stream = Stream(
            node_name=node_name,
            stream_name=stream_name,
            pipe=pipe,
        )
        self._streams.append(stream)
        return stream

    async def mark_finished(self, node_name: str, stream_name: Optional[str]) -> None:
        """Mark a stream as finished."""
        stream = self.get(node_name, stream_name)
        await stream.pipe.get_writer().close()

    @staticmethod
    def make_env_stream(params: Dict[str, Any]) -> Stream:
        content = json.dumps(params).encode("utf-8")
        buf = AsyncBuffer(
            initial_content=content, is_closed=True, on_write_started=lambda: None
        )
        return Stream(
            node_name=".",
            stream_name="env",
            buf=buf,
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
