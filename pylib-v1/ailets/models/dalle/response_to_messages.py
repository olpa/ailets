import json
from typing import Optional
from ailets.atyping import (
    ChatMessage,
    ContentItemImage,
    ContentItemImageAttrs,
    ContentItemImageContent,
    ContentItemText,
    Content,
    ContentItemTextAttrs,
    ContentItemTextContent,
    INodeRuntime,
    StdHandles,
)
from ailets.io.input_reader import iter_input_objects
from ailets.cons.util import write_all


async def response_to_messages(runtime: INodeRuntime) -> None:
    """Convert DALL-E response to messages."""

    async for response in iter_input_objects(runtime, StdHandles.stdin):
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
            text: Optional[ContentItemText] = None
            if revised_prompt := item.get("revised_prompt"):
                t0: ContentItemTextAttrs = {"type": "text"}
                t1: ContentItemTextContent = {"text": revised_prompt}
                text = (t0, t1)

            assert (
                "url" in item or "b64_json" in item
            ), 'Invalid response. "data" item should contain either "url" or "b64_json"'
            url = (
                item["url"]
                if "url" in item
                else f"data:image/png;base64,{item['b64_json']}"
            )

            i0: ContentItemImageAttrs = {
                "type": "image",
                "content_type": item.get("content_type"),
            }
            i1: ContentItemImageContent = {"image_url": url}
            image: ContentItemImage = (i0, i1)

            content: Content = [text, image] if text else [image]
            message: ChatMessage = {
                "role": "assistant",
                "content": content,
            }
            await write_all(
                runtime,
                StdHandles.stdout,
                json.dumps(message).encode("utf-8"),
            )
