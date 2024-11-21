import json
from typing import Sequence
from ailets.cons.typing import ChatMessage, INodeRuntime


def messages_to_markdown(runtime: INodeRuntime) -> None:
    """Convert chat messages to markdown."""
    output = runtime.open_write(None)

    need_separator = False

    # `n_of_streams` can change with time, therefore don't use `range`
    i = 0
    while i < runtime.n_of_streams(None):
        messages: Sequence[ChatMessage] = json.loads(runtime.open_read(None, i).read())
        i += 1

        for message in messages:
            if need_separator:
                output.write("\n\n")

            mixed_content = message["content"]
            if isinstance(mixed_content, str):
                output.write(mixed_content)
            else:
                json.dump(mixed_content, output)

            need_separator = True

    runtime.close_write(None)
