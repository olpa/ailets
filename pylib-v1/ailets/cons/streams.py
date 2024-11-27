import base64
from dataclasses import dataclass
import io
import json
from typing import Any, Dict, Optional, Sequence, TextIO
from io import BytesIO
from typing_extensions import Buffer

from ailets.cons.typing import Dependency, IStream


@dataclass
class Stream:
    """A stream of data associated with a node.

    Attributes:
        node_name: Name of the node this stream belongs to
        stream_name: Name of the stream
        is_finished: Whether the stream is complete
        content: The BytesIO buffer containing the stream data
    """

    node_name: str
    stream_name: Optional[str]
    is_finished: bool
    content: BytesIO

    def get_content(self) -> BytesIO:
        return self.content

    def to_json(self) -> dict:
        """Convert stream to JSON-serializable dict."""
        b = self.content.getvalue()
        try:
            content_field = "content"
            content = b.decode("utf-8")
        except UnicodeDecodeError:
            content_field = "b64_content"
            content = base64.b64encode(b).decode("utf-8")
        return {
            "node": self.node_name,
            "name": self.stream_name,
            "is_finished": self.is_finished,
            content_field: content,
        }

    @classmethod
    def from_json(cls, data: dict) -> "Stream":
        """Create stream from JSON data."""
        if "b64_content" in data:
            content = base64.b64decode(data["b64_content"])
        else:
            content = data["content"].encode("utf-8")
        return cls(
            node_name=data["node"],
            stream_name=data["name"],
            is_finished=data["is_finished"],
            content=BytesIO(content),
        )

    def close(self) -> None:
        self.is_finished = True


def create_log_stream() -> Stream:
    class LogStream(io.BytesIO):
        def write(self, b: Buffer) -> int:
            if isinstance(b, bytes):
                b2 = b.decode("utf-8")
            else:
                b2 = str(b)
            print(b2, end="")
            return len(b2)

    return Stream(
        node_name=".",
        stream_name="log",
        is_finished=False,
        content=LogStream(),
    )


class Streams:
    """Manages streams for an environment."""

    def __init__(self) -> None:
        self._streams: list[Stream] = []

    def _find_stream(
        self, node_name: str, stream_name: Optional[str]
    ) -> Optional[Stream]:
        """Find a stream by node name and stream name.

        Args:
            node_name: Name of the node
            stream_name: Name of the stream

        Returns:
            The stream if found, None otherwise
        """
        return next(
            (
                s
                for s in self._streams
                if s.node_name == node_name and s.stream_name == stream_name
            ),
            None,
        )

    def get(self, node_name: str, stream_name: Optional[str]) -> Stream:
        """Get a stream by node name and stream name."""
        stream = self._find_stream(node_name, stream_name)
        if stream is None:
            raise ValueError(f"Stream not found: {node_name}.{stream_name}")
        return stream

    def create(self, node_name: str, stream_name: Optional[str]) -> Stream:
        """Add a new stream."""
        if stream_name == "log":
            return create_log_stream()

        if self._find_stream(node_name, stream_name) is not None:
            raise ValueError(f"Stream already exists: {node_name}.{stream_name}")

        stream = Stream(
            node_name=node_name,
            stream_name=stream_name,
            is_finished=False,
            content=BytesIO(),
        )
        self._streams.append(stream)
        return stream

    def mark_finished(self, node_name: str, stream_name: Optional[str]) -> None:
        """Mark a stream as finished."""
        stream = self.get(node_name, stream_name)
        stream.close()

    def to_json(self, f: TextIO) -> None:
        """Convert all streams to JSON-serializable format."""
        for stream in self._streams:
            json.dump(stream.to_json(), f, indent=2)
            f.write("\n")

    def add_stream_from_json(self, stream_data: dict) -> Stream:
        """Load a stream's state from JSON data."""
        stream = Stream.from_json(stream_data)
        self._streams.append(stream)
        return stream

    @staticmethod
    def make_env_stream(params: Dict[str, Any]) -> Stream:
        return Stream(
            node_name=".",
            stream_name="env",
            is_finished=True,
            content=BytesIO(json.dumps(params).encode("utf-8")),
        )

    def get_fs_output_streams(self) -> Sequence[Stream]:
        return [
            s
            for s in self._streams
            if s.stream_name is not None and s.stream_name.startswith("./out/")
        ]

    def collect_streams(
        self,
        deps: Sequence[Dependency],
    ) -> Sequence[IStream]:
        collected: list[IStream] = []
        for dep in deps:
            collected.extend(
                s
                for s in self._streams
                if s.node_name == dep.source and s.stream_name == dep.stream
            )
        return collected

    def read_dir(self, dir_name: str, node_names: Sequence[str]) -> Sequence[str]:
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

    def pass_through(
        self,
        node_name: str,
        in_streams: Sequence[IStream],
        out_stream_name: str,
    ) -> None:
        for in_stream in in_streams:
            out_stream = Stream(
                node_name=node_name,
                stream_name=out_stream_name,
                is_finished=True,
                content=BytesIO(in_stream.get_content().getvalue()),
            )
            self._streams.append(out_stream)
