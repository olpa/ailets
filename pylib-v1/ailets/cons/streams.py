from dataclasses import dataclass
import json
from typing import Any, Dict, Optional, Sequence

from ailets.cons.atyping import Dependency, IStream
from ailets.cons.async_buf import AsyncBuffer


@dataclass
class Stream:
    node_name: str
    stream_name: Optional[str]
    buf: AsyncBuffer

    async def read(self, pos: int, size: int = -1) -> bytes:
        return await self.buf.read(pos, size)

    async def write(self, data: bytes) -> int:
        return await self.buf.write(data)

    async def close(self) -> None:
        await self.buf.close()

    def get_name(self) -> Optional[str]:
        return self.stream_name

    def is_closed(self) -> bool:
        return self.buf.is_closed()


def create_log_stream() -> Stream:
    class LogStream(AsyncBuffer):
        async def write(self, b: bytes) -> int:
            b2 = b.decode("utf-8")
            print(b2, end="")
            return len(b2)

    return Stream(
        node_name=".",
        stream_name="log",
        buf=LogStream(),
    )


class Streams:
    """Manages streams for an environment."""

    def __init__(self) -> None:
        self._streams: list[Stream] = []

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

        stream = Stream(
            node_name=node_name,
            stream_name=stream_name,
            buf=AsyncBuffer(initial_content=initial_content, is_closed=is_closed),
        )
        self._streams.append(stream)
        return stream

    async def mark_finished(self, node_name: str, stream_name: Optional[str]) -> None:
        """Mark a stream as finished."""
        stream = self.get(node_name, stream_name)
        await stream.close()

    @staticmethod
    def make_env_stream(params: Dict[str, Any]) -> Stream:
        content = json.dumps(params).encode("utf-8")
        buf = AsyncBuffer(initial_content=content, is_closed=True)
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
    ) -> Sequence[IStream]:
        collected: list[IStream] = []
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
