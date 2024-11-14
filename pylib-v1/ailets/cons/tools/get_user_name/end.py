from ailets.cons.typing import INodeRuntime


def end(runtime: INodeRuntime) -> None:
    """End the get_user_name tool."""
    runtime.forward_stream(None, None)
