import os
from ailets.cons.typing import INodeRuntime


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


def call_get_user_name(runtime: INodeRuntime) -> None:
    """Call the get_user_name tool."""
    value = os.environ["USER"]
    runtime.open_write(None).write(value)
    runtime.close_write(None)

    runtime.open_write("type").write("text")
    runtime.close_write("type")
