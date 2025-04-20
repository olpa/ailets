import dbm.gnu
import base64
from typing import Literal, Dict, Sequence
from ..atyping import IKVBuffer, IKVBuffers


class GdbmKVBuffer(IKVBuffer):
    def __init__(self, path: str, shared_buffer: bytearray):
        self.buffer = shared_buffer
        self.path = path

    def borrow_mut_buffer(self) -> bytearray:
        return self.buffer


class GdbmKV(IKVBuffers):
    def __init__(self, dbpath: str) -> None:
        self._buffers: Dict[str, bytearray] = {}
        # "c": Open database for reading and writing, creating it if it doesn’t exist
        # "s": Synchronized mode: immediately write changes to the file
        self._db = dbm.gnu.open(dbpath, "cs")

    def destroy(self) -> None:
        self._db.sync()
        self._db.close()

    def open(self, path: str, mode: Literal["read", "write", "append"]) -> IKVBuffer:
        if path not in self._buffers:
            value = self._db.get(path)
            if value is not None:
                self._buffers[path] = bytearray(base64.b64decode(value))
        if mode == "read":
            if path not in self._buffers:
                raise KeyError(f"Path not found: {path}")
            return GdbmKVBuffer(path, self._buffers[path])
        if mode == "write":
            buffer = bytearray()
            self._buffers[path] = buffer
            return GdbmKVBuffer(path, buffer)
        if mode == "append":
            buffer = self._buffers.get(path, bytearray())
            self._buffers[path] = buffer
            return GdbmKVBuffer(path, buffer)
        raise ValueError(f"Invalid mode: {mode}")

    def flush(self, kvbuffer: IKVBuffer) -> None:
        if isinstance(kvbuffer, GdbmKVBuffer):
            self._db[kvbuffer.path] = base64.b64encode(kvbuffer.buffer)

    def listdir(self, dir_name: str) -> Sequence[str]:
        if dir_name and not dir_name.endswith("/"):
            dir_name = dir_name + "/"
        return [path for path in self._buffers.keys() if path.startswith(dir_name)]
