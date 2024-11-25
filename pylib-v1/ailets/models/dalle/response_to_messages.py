import json
from typing import Optional
from ailets.cons.typing import (
    ChatMessageAssistant,
    ChatMessageContentImageUrl,
    ChatMessageContentText,
    ChatMessageStructuredContent,
    INodeRuntime,
)
from ailets.cons.util import iter_streams_objects, write_all


def response_to_messages(runtime: INodeRuntime) -> None:
    """Convert DALL-E response to messages."""

    output_fd = runtime.open_write(None)

    for response in iter_streams_objects(runtime, None):
        # `response` format:
        # {
        #   "created": 1726961295,
        #   "data": [
        #     {
        #       "url": "https://...",
        #       "revised_prompt": "..."
        #     }
        #   ]
        # }

        for item in response["data"]:
            text: Optional[ChatMessageContentText] = (
                {
                    "type": "text",
                    "text": item["revised_prompt"],
                }
                if item.get("revised_prompt")
                else None
            )

            assert (
                "url" in item or "b64_json" in item
            ), 'Invalid response. "data" item should contain either "url" or "b64_json"'
            url = (
                item["url"]
                if "url" in item
                else f"data:image/png;base64,{item['b64_json']}"
            )

            image: ChatMessageContentImageUrl = {
                "type": "image_url",
                "image_url": {
                    "url": url,
                },
            }

            content: ChatMessageStructuredContent = [text, image] if text else [image]
            message: ChatMessageAssistant = {
                "role": "assistant",
                "content": content,
            }
            write_all(runtime, output_fd, json.dumps(message).encode("utf-8"))

    runtime.close(output_fd)
