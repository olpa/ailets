import dbm.gnu
import base64
from typing import Literal
from ailets.atyping import IKVBuffer
from .basekv import KVBuffers, KVBuffer


class GdbmKV(KVBuffers):
    def __init__(self, dbpath: str) -> None:
        super().__init__()
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
        return super().open(path, mode)

    def flush(self, kvbuffer: IKVBuffer) -> None:
        if isinstance(kvbuffer, KVBuffer):
            self._db[kvbuffer.path] = base64.b64encode(kvbuffer.buffer)
