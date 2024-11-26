import base64
import json
from ailets.cons.typeguards import is_chat_message_system
from ailets.cons.typing import (
    ChatMessage,
    ChatMessageStructuredContentItem,
    INodeRuntime,
)
from ailets.cons.util import iter_streams_objects, read_all, write_all
from ailets.models.gpt4o.typing import Gpt4oChatMessageContentItem

url = "https://api.openai.com/v1/chat/completions"
method = "POST"
headers = {
    "Content-type": "application/json",
    "Authorization": "Bearer {{secret('openai','gpt4o')}}",
}


def rewrite_content_item(
    runtime: INodeRuntime,
    item: ChatMessageStructuredContentItem,
) -> Gpt4oChatMessageContentItem:
    if item["type"] == "text":
        return item
    assert item["type"] == "image", "Only text and image are supported"

    if url := item.get("url"):
        return {
            "type": "image_url",
            "image_url": {
                "url": url,
            },
        }

    stream = item.get("stream")
    assert stream, "Image URL or stream is required"

    fd = runtime.open_read(stream, 0)
    data = read_all(runtime, fd)
    runtime.close(fd)

    b64_data = base64.b64encode(data).decode('utf-8')
    data_url = f"data:{item['content_type']};base64,{b64_data}"
    return {
        "type": "image_url",
        "image_url": {"url": data_url},
    }


def messages_to_query(runtime: INodeRuntime) -> None:
    """Convert chat messages into a query."""

    messages: list[ChatMessage] = []
    for message in iter_streams_objects(runtime, None):
        if is_chat_message_system(message):
            messages.append(message)
            continue

        new_content = [
            rewrite_content_item(runtime, item)
            for item in message["content"]
        ]
        new_message: ChatMessage = message.copy()  # type: ignore[assignment]
        new_message["content"] = new_content  # type: ignore[arg-type]

        messages.append(new_message)

    tools = []
    for toolspec in iter_streams_objects(runtime, "toolspecs"):
        tools.append(
            {
                "type": "function",
                "function": toolspec,
            }
        )
    tools_param = {"tools": tools} if tools else {}

    value = {
        "url": url,
        "method": method,
        "headers": headers,
        "body": {
            "model": "gpt-4o-mini",
            "messages": messages,
            **tools_param,
        },
    }

    fd = runtime.open_write(None)
    write_all(runtime, fd, json.dumps(value).encode("utf-8"))
    runtime.close(fd)
