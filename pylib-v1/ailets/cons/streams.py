from dataclasses import dataclass
import json
from typing import Any, Dict, Optional, TextIO
from io import BytesIO


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

    def to_json(self) -> dict:
        """Convert stream to JSON-serializable dict."""
        return {
            "node": self.node_name,
            "name": self.stream_name,
            "is_finished": self.is_finished,
            "content": self.content.getvalue().decode("utf-8"),
        }

    @classmethod
    def from_json(cls, data: dict) -> "Stream":
        """Create stream from JSON data."""
        return cls(
            node_name=data["node"],
            stream_name=data["name"],
            is_finished=data["is_finished"],
            content=BytesIO(data["content"].encode("utf-8")),
        )

    def close(self) -> None:
        self.is_finished = True


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
