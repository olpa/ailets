import json
from typing import Optional
from ailets.cons.typing import (
    ChatMessageAssistant,
    ChatMessageContentImageUrl,
    ChatMessageContentText,
    ChatMessageStructuredContent,
    INodeRuntime,
)


def response_to_image(runtime: INodeRuntime) -> None:
    """Convert DALL-E response to image."""

    output = runtime.open_write(None)

    for i in range(runtime.n_of_streams(None)):
        response = json.loads(runtime.open_read(None, i).read())
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
            image: ChatMessageContentImageUrl = {
                "type": "image_url",
                "image_url": {
                    "url": item["url"],
                },
            }
            content: ChatMessageStructuredContent = [text, image] if text else [image]

            message: ChatMessageAssistant = {
                "role": "assistant",
                "content": content,
            }
            output.write(json.dumps(message))

    runtime.close_write(None)
