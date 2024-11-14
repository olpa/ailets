import os
from ailets.cons.typing import INodeRuntime


def begin(runtime: INodeRuntime) -> None:
    """Call the get_user_name tool."""
    value = os.environ["USER"]

    runtime.open_write(None).write(value)
    runtime.close_write(None)
