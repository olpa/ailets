from typing import Literal, Dict, Sequence
from .atyping import IKVBuffer, IKVBuffers


class MemoryKVBuffer(IKVBuffer):
    def __init__(self, path: str, initial_content: bytes = b""):
        self.buffer = bytearray(initial_content)
        self._path = path

    def borrow_mut_buffer(self) -> bytearray:
        return self.buffer


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

    def read_dir(self, dir_name: str) -> Sequence[str]:
        if not dir_name.endswith("/"):
            dir_name = dir_name + "/"
        return [path for path in self._buffers.keys() if path.startswith(dir_name)]
