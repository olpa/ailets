from ailets.cons.typing import NodeDesc, Dependency
from .call import schema

call = NodeDesc(
    name="call",
    inputs=[
        Dependency(source="caller", schema=schema),
    ],
)

nodes = [call]
