from typing import Literal
from ailets.atyping import IKVBuffer
from .basekv import KVBuffers, KVBuffer
from ailets.io.dbm_sqlite3 import open as dbm_open  # type: ignore[attr-defined]

class SqliteKV(KVBuffers):
    def __init__(self, dbpath: str) -> None:
        super().__init__()
        # "c": Open database for reading and writing, creating it if it doesn’t exist
        self._db = dbm_open(dbpath, "c")

    def destroy(self) -> None:
        self._db.close()

    def open(self, path: str, mode: Literal["read", "write", "append"]) -> IKVBuffer:
        if path not in self._buffers:
            value = self._db.get(path)
            if value is not None:
                self._buffers[path] = bytearray(value)
        return super().open(path, mode)

    def flush(self, kvbuffer: IKVBuffer) -> None:
        if isinstance(kvbuffer, KVBuffer):
            self._db[kvbuffer.path] = kvbuffer.buffer
