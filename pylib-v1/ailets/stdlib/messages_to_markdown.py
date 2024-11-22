import json
from ailets.cons.typing import INodeRuntime
from ailets.cons.util import iter_streams_objects

need_separator = False


def messages_to_markdown(runtime: INodeRuntime) -> None:
    """Convert chat messages to markdown."""
    global need_separator
    need_separator = False

    output = runtime.open_write(None)

    for message in iter_streams_objects(runtime):
        if need_separator:
            output.write("\n\n")

        mixed_content = message["content"]
        if isinstance(mixed_content, str):
            output.write(mixed_content)
        else:
            json.dump(mixed_content, output)

        need_separator = True

    runtime.close_write(None)
