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

begin = NodeDesc(
    name="begin",
    inputs=[
        Dependency(source="caller", schema=schema),
    ],
)

end = NodeDesc(
    name="end",
    inputs=[
        Dependency(source="begin"),
    ],
)

nodes = [begin, end]
