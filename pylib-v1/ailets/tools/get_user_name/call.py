import os
from ailets.cons.typing import INodeRuntime
from ailets.cons.util import write_all


def call(runtime: INodeRuntime) -> None:
    """Call the get_user_name tool."""
    value = os.environ["USER"]

    fd = runtime.open_write(None)
    write_all(runtime, fd, value.encode("utf-8"))
    runtime.close(fd)
