from ailets.cons.typing import NodeDesc, Dependency

schema = {
    "name": "get_user_name",
    "description": (
        "Get the user's name. Call this whenever you need to know the name "
        "of the user."
    ),
    "strict": True,
    "parameters": {
        "type": "object",
        "properties": {},
        "additionalProperties": False,
    },
}

call = NodeDesc(
    name="call",
    inputs=[
        Dependency(source="input", schema=schema),
    ],
)

nodes = [call]
