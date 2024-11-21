import json
from ailets.cons.typing import INodeRuntime


def messages_to_markdown(runtime: INodeRuntime) -> None:
    """Convert chat messages to markdown."""
    output = runtime.open_write(None)

    # `n_of_streams` can change with time, therefore don't use `range`
    i = 0
    while i < runtime.n_of_streams(None):
        messages = json.loads(runtime.open_read(None, i).read())
        i += 1

        if i > 0:
            output.write("\n\n")

        output.write(str(messages))

    runtime.close_write(None)
