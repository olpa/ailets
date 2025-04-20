from typing import Literal, Dict, Sequence
from ailets.atyping import IKVBuffer, IKVBuffers


class MemKVBuffer(IKVBuffer):
    def __init__(self, path: str, shared_buffer: bytearray):
        self.buffer = shared_buffer
        self._path = path

    def borrow_mut_buffer(self) -> bytearray:
        return self.buffer


class MemKV(IKVBuffers):
    def __init__(self) -> None:
        self._buffers: Dict[str, bytearray] = {}

    def open(self, path: str, mode: Literal["read", "write", "append"]) -> IKVBuffer:
        if mode == "read":
            if path not in self._buffers:
                raise KeyError(f"Path not found: {path}")
            return MemKVBuffer(path, self._buffers[path])
        if mode == "write":
            buffer = bytearray()
            self._buffers[path] = buffer
            return MemKVBuffer(path, buffer)
        if mode == "append":
            buffer = self._buffers.get(path, bytearray())
            self._buffers[path] = buffer
            return MemKVBuffer(path, buffer)
        raise ValueError(f"Invalid mode: {mode}")

    def flush(self, kvbuffer: IKVBuffer) -> None:
        pass

    def listdir(self, dir_name: str) -> Sequence[str]:
        if dir_name and not dir_name.endswith("/"):
            dir_name = dir_name + "/"
        return [path for path in self._buffers.keys() if path.startswith(dir_name)]
