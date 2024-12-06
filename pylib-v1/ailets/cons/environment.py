from typing import Any, Dict
from ailets.cons.atyping import IEnvironment, INodeRegistry
from ailets.cons.dagops import Dagops
from ailets.cons.processes import Processes
from ailets.cons.seqno import Seqno
from ailets.cons.streams import Streams


class Environment(IEnvironment):
    def __init__(self, nodereg: INodeRegistry) -> None:
        self.for_env_stream: Dict[str, Any] = {}
        self.seqno = Seqno()
        self.dagops = Dagops(self.seqno)
        self.streams = Streams()
        self.nodereg = nodereg
        self.processes = Processes(self)
