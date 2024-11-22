import json
from typing import TextIO
from ailets.cons.typeguards import (
    is_chat_message_content_image_url,
    is_chat_message_content_text,
)
from ailets.cons.typing import ChatMessageStructuredContentItem, INodeRuntime
from ailets.cons.util import iter_streams_objects

need_separator = False


def separator(output: TextIO) -> None:
    global need_separator
    if need_separator:
        output.write("\n\n")
    else:
        need_separator = True


def mixed_content_to_markdown(
    content: ChatMessageStructuredContentItem, output: TextIO
) -> None:
    separator(output)

    if isinstance(content, str):
        output.write(content)
        return

    if is_chat_message_content_text(content):
        output.write(content["text"])
        return

    if is_chat_message_content_image_url(content):
        output.write(f"![image]({content['image_url']['url']})")
        return

    json.dump(content, output)


def messages_to_markdown(runtime: INodeRuntime) -> None:
    """Convert chat messages to markdown."""
    global need_separator
    need_separator = False

    output = runtime.open_write(None)

    for message in iter_streams_objects(runtime):
        content = message["content"]
        if isinstance(content, str):
            separator(output)
            output.write(content)
            continue
        for item in content:
            mixed_content_to_markdown(item, output)

    runtime.close_write(None)
