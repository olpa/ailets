from ailets.cons.typing import INodeRuntime


def end(runtime: INodeRuntime) -> None:
    """End the get_user_name tool."""
    output = runtime.open_write(None)

    i = 0
    while content := runtime.open_read(None, i).read():
        i += 1
        output.write(content)

    runtime.close_write(None)
