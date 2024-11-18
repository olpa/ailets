from ailets.cons.typing import INodeRuntime


def stdout(runtime: INodeRuntime) -> None:
    """Print each value to stdout and return them unchanged."""

    for i in range(runtime.n_of_streams(None)):
        value = runtime.open_read(None, i).read()
        if value == "":
            continue
        print(value)

    runtime.open_write(None).write("ok")
    runtime.close_write(None)
