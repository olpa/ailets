from typing import Any, Dict
from ailets.cons.dagops import Dagops
from ailets.cons.plugin import NodeRegistry
from ailets.cons.seqno import Seqno
from ailets.cons.streams import Streams


class Environment:
    def __init__(self):
        self.for_env_stream: Dict[str, Any] = {}
        self.seqno = Seqno()
        self.dagops = Dagops()
        self.streams = Streams()
        self.nodereg = NodeRegistry()
