from typing import Any, Dict
from ailets.cons.atyping import IEnvironment, INodeRegistry, Errors
from ailets.cons.dagops import Dagops
from ailets.cons.notification_queue import NotificationQueue
from ailets.cons.processes import Processes
from ailets.cons.seqno import Seqno
from ailets.cons.piper import Piper
from ailets.cons.memkv import MemKV


class Environment(IEnvironment):
    def __init__(self, nodereg: INodeRegistry) -> None:
        self.for_env_pipe: Dict[str, Any] = {}
        self.errno: Errors = Errors.NoError

        self.seqno = Seqno()
        self.kv = MemKV()
        self.dagops = Dagops(self.seqno)
        self.notification_queue = NotificationQueue()
        self.piper = Piper(self.kv, self.notification_queue, self.seqno)
        self.nodereg = nodereg
        self.processes = Processes(self)

    def destroy(self) -> None:
        self.processes.destroy()
        self.piper.destroy()

    def get_errno(self) -> Errors:
        return self.errno

    def set_errno(self, errno: Errors) -> None:
        self.errno = errno
