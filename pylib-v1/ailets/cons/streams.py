from dataclasses import dataclass
from typing import Optional, Sequence
from io import StringIO


@dataclass
class Stream:
    """A stream of data associated with a node.

    Attributes:
        node_name: Name of the node this stream belongs to
        stream_name: Name of the stream
        is_finished: Whether the stream is complete
        content: The StringIO buffer containing the stream data
    """

    node_name: str
    stream_name: str
    is_finished: bool
    content: StringIO

    def to_json(self) -> dict:
        """Convert stream to JSON-serializable dict."""
        return {
            "node": self.node_name,
            "name": self.stream_name,
            "finished": self.is_finished,
            "content": self.content.getvalue(),
        }

    @classmethod
    def from_json(cls, data: dict) -> "Stream":
        """Create stream from JSON data."""
        return cls(
            node_name=data["node"],
            stream_name=data["name"],
            is_finished=data["finished"],
            content=StringIO(data["content"]),
        )


class Streams:
    """Manages streams for an environment."""

    def __init__(self):
        self._streams: list[Stream] = []

    @property
    def streams(self) -> Sequence[Stream]:
        """Get all streams."""
        return self._streams

    def _find_stream(self, node_name: str, stream_name: str) -> Optional[Stream]:
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

    def get(self, node_name: str, stream_name: str) -> Stream:
        """Get a stream by node name and stream name."""
        stream = self._find_stream(node_name, stream_name)
        if stream is None:
            raise ValueError(f"Stream not found: {node_name}.{stream_name}")
        return stream

    def create(self, node_name: str, stream_name: str) -> Stream:
        """Add a new stream."""
        if self._find_stream(node_name, stream_name) is not None:
            raise ValueError(f"Stream already exists: {node_name}.{stream_name}")

        stream = Stream(
            node_name=node_name,
            stream_name=stream_name,
            is_finished=False,
            content=StringIO(),
        )
        self._streams.append(stream)
        return stream

    def mark_finished(self, node_name: str, stream_name: str) -> None:
        """Mark a stream as finished."""
        stream = self.get(node_name, stream_name)
        stream.is_finished = True

    def to_json(self) -> list[dict]:
        """Convert all streams to JSON-serializable format."""
        return [stream.to_json() for stream in self._streams]

    def load_state(self, stream_data: dict) -> Stream:
        """Load a stream's state from JSON data."""
        if not all(k in stream_data for k in ["node", "name", "finished", "content"]):
            raise ValueError("Invalid stream data: missing required fields")

        stream = Stream(
            node_name=stream_data["node"],
            stream_name=stream_data["name"],
            is_finished=stream_data["finished"],
            content=StringIO(stream_data["content"]),
        )

        self._streams.append(stream)
        return stream
