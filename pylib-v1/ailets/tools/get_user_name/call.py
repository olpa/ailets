import os
from ailets.cons.typing import INodeRuntime


def call(runtime: INodeRuntime) -> None:
    """Call the get_user_name tool."""
    value = os.environ["USER"]

    runtime.open_write(None).write(value.encode("utf-8"))
    runtime.close_write(None)
