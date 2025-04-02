from typing import Literal, Dict
from .atyping import IKVBuffer, IKVBuffers


class MemoryKVBuffer(IKVBuffer):
    def __init__(self, path: str, initial_buffer: bytes = b""):
        self.buffer = initial_buffer
        self._path = path


class MemoryKVBuffers(IKVBuffers):
    def __init__(self) -> None:
        self._buffers: Dict[str, bytes] = {}

    def open(self, path: str, mode: Literal["read", "write", "append"]) -> IKVBuffer:
        if mode == "read":
            if path not in self._buffers:
                raise KeyError(f"Path not found: {path}")
            return MemoryKVBuffer(path, self._buffers[path])
        if mode == "write":
            return MemoryKVBuffer(path, b"")
        if mode == "append":
            return MemoryKVBuffer(path, self._buffers.get(path, b""))
        raise ValueError(f"Invalid mode: {mode}")

    def flush(self, kvbuffer: IKVBuffer) -> None:
        pass
