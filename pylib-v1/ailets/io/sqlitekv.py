import threading
from typing import Dict, Literal
from ailets.atyping import IKVBuffer
from .basekv import KVBuffers, KVBuffer
from ailets.io.dbm_sqlite3 import open as dbm_open  # type: ignore[attr-defined]

# In Python 3.10, sqlite3.Connection threading mode is hardcoded:
# > Threads may share the module, but not connections
# It applies not only to `get` and `set`, but also to `close`
# Considering that `SqliteKV` is a debug tool, we allow ourselves
# leave some connections open.
class Dbwrapper:
    def __init__(self, dbpath: str) -> None:
        self._dbpath = dbpath
        self._thread_to_db: Dict[int, Dict[str, bytes]] = {}

    def get_db(self) -> Dict[str, bytes]:
        tid = threading.get_native_id()
        db = self._thread_to_db.get(tid)
        if db is None:
            # "c": Open database for reading and writing, creating it if it doesn’t exist
            db = dbm_open(self._dbpath, "c")
            self._thread_to_db[tid] = db
        return db

    def close(self) -> None:
        tid = threading.get_native_id()
        db = self._thread_to_db.get(tid)
        if db is not None:
            db.close()  # type: ignore[attr-defined]


class SqliteKV(KVBuffers):
    def __init__(self, dbpath: str) -> None:
        super().__init__()
        self._db = Dbwrapper(dbpath)

    def destroy(self) -> None:
        self._db.close()

    def open(self, path: str, mode: Literal["read", "write", "append"]) -> IKVBuffer:
        if path not in self._buffers:
            db = self._db.get_db()
            value = db.get(path)
            if value is not None:
                self._buffers[path] = bytearray(value)
        return super().open(path, mode)

    def flush(self, path: str) -> None:
        buf = self._buffers.get(path)
        if buf is None:
            return
        db = self._db.get_db()
        db[path] = buf
