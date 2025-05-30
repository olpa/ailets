import json
from ailets.atyping import (
    ContentItemCtl,
    ContentItemImage,
    ContentItemImageAttrs,
    ContentItemImageContent,
    ContentItemText,
    ContentItemTextAttrs,
    ContentItemTextContent,
    INodeRuntime,
    StdHandles,
)
from ailets.io.input_reader import iter_input_objects
from ailets.cons.util import write_all


async def response_to_messages(runtime: INodeRuntime) -> None:
    """Convert DALL-E response to messages."""

    ctl: ContentItemCtl = ({"type": "ctl"}, {"role": "assistant"})
    await write_all(runtime, StdHandles.stdout, json.dumps(ctl).encode("utf-8"))

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
            if revised_prompt := item.get("revised_prompt"):
                t0: ContentItemTextAttrs = {"type": "text"}
                t1: ContentItemTextContent = {"text": revised_prompt}
                text: ContentItemText = (t0, t1)
                await write_all(
                    runtime, StdHandles.stdout, json.dumps(text).encode("utf-8")
                )
                continue

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
            await write_all(
                runtime, StdHandles.stdout, json.dumps(image).encode("utf-8")
            )
