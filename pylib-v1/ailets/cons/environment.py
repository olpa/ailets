from typing import Any, Dict
from ailets.atyping import IEnvironment, INodeRegistry
from ailets.cons.dagops import Dagops
from ailets.cons.notification_queue import NotificationQueue
from ailets.cons.processes import Processes
from ailets.cons.seqno import Seqno
from ailets.io.piper import Piper
from ailets.io.memkv import MemKV


class Environment(IEnvironment):
    def __init__(self, nodereg: INodeRegistry) -> None:
        self.for_env_pipe: Dict[str, Any] = {}
        self.errno: int = 0

        self.seqno = Seqno()
        for _ in range(10):  # To avoid collision with StdHandles
            self.seqno.next_seqno()

        self.kv = MemKV()
        self.dagops = Dagops(self.seqno)
        self.notification_queue = NotificationQueue()
        self.piper = Piper(self.kv, self.notification_queue, self.seqno)
        self.nodereg = nodereg
        self.processes = Processes(self)

    def destroy(self) -> None:
        self.processes.destroy()
        self.piper.destroy()

    def get_errno(self) -> int:
        return self.errno

    def set_errno(self, errno: int) -> None:
        self.errno = errno
