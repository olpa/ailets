import json
from typing import Optional
from ailets.cons.atyping import (
    ChatMessage,
    ContentItemImage,
    ContentItemText,
    Content,
    INodeRuntime,
)
from ailets.cons.util import iter_input_objects, write_all


async def response_to_messages(runtime: INodeRuntime) -> None:
    """Convert DALL-E response to messages."""

    output_fd = await runtime.open_write("")

    async for response in iter_input_objects(runtime, ""):
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
            text: Optional[ContentItemText] = (
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

            image: ContentItemImage = {
                "type": "image",
                "content_type": "image/png",
                "url": url,
            }

            content: Content = [text, image] if text else [image]
            message: ChatMessage = {
                "role": "assistant",
                "content": content,
            }
            await write_all(runtime, output_fd, json.dumps(message).encode("utf-8"))

    await runtime.close(output_fd)
