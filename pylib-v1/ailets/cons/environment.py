from typing import Any, Dict
from ailets.cons.atyping import IEnvironment, INodeRegistry
from ailets.cons.dagops import Dagops
from ailets.cons.notification_queue import NotificationQueue
from ailets.cons.processes import Processes
from ailets.cons.seqno import Seqno
from ailets.cons.piper import Piper
from ailets.cons.memory_kv_buffers import MemoryKVBuffers


class Environment(IEnvironment):
    def __init__(self, nodereg: INodeRegistry) -> None:
        self.for_env_stream: Dict[str, Any] = {}
        self.seqno = Seqno()
        self.kv = MemoryKVBuffers()
        self.dagops = Dagops(self.seqno)
        self.notification_queue = NotificationQueue()
        self.piper = Piper(self.kv, self.notification_queue, self.seqno)
        self.nodereg = nodereg
        self.processes = Processes(self)

    def destroy(self) -> None:
        self.processes.destroy()
        self.piper.destroy()
