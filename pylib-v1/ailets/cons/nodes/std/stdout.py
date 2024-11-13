import json
from ..node_runtime import NodeRuntime


def stdout(runtime: NodeRuntime) -> None:
    """Print each value to stdout and return them unchanged."""

    for i in range(runtime.n_of_streams(None)):
        value = json.loads(runtime.open_read(None, i).read())
        print(value)

    runtime.open_write(None).write("ok")
    runtime.close_write(None)
