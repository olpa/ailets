from typing import Literal, Dict, Sequence
from ailets.atyping import IKVBuffer, IKVBuffers


class KVBuffer(IKVBuffer):
    def __init__(self, path: str, shared_buffer: bytearray):
        self.buffer = shared_buffer
        self.path = path

    def borrow_mut_buffer(self) -> bytearray:
        return self.buffer


class KVBuffers(IKVBuffers):
    def __init__(self) -> None:
        self._buffers: Dict[str, bytearray] = {}
    
    def destroy(self) -> None:
        self._buffers.clear()

    def open(self, path: str, mode: Literal["read", "write", "append"]) -> IKVBuffer:
        if mode == "read":
            if path not in self._buffers:
                raise KeyError(f"Path not found: {path}")
            return KVBuffer(path, self._buffers[path])
        if mode == "write":
            buffer = bytearray()
            self._buffers[path] = buffer
            return KVBuffer(path, buffer)
        if mode == "append":
            buffer = self._buffers.get(path, bytearray())
            self._buffers[path] = buffer
            return KVBuffer(path, buffer)
        raise ValueError(f"Invalid mode: {mode}")

    def flush(self, path: str) -> None:
        pass

    def listdir(self, dir_name: str) -> Sequence[str]:
        if dir_name and not dir_name.endswith("/"):
            dir_name = dir_name + "/"
        return [path for path in self._buffers.keys() if path.startswith(dir_name)]
