from ailets.cons.dagops import Dagops
from ailets.cons.streams import Streams

class Environment:
    def __init__(self):
        self.dagops = Dagops()
        self.streams = Streams()
