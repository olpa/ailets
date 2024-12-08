import json
from typing import Any, List, Optional
from ailets.cons.typeguards import (
    is_content_item_refusal,
    is_content_item_text,
)
from ailets.cons.atyping import (
    ChatMessage,
    Content,
    ContentItem,
    ContentItemImage,
    ContentItemText,
    INodeRuntime,
)
from ailets.cons.util import (
    iter_streams_objects,
    write_all,
)
from ailets.models.gpt4o.lib.sse import SseHandler, is_sse_object
from ailets.models.gpt4o.lib.typing import (
    Gpt4oMessage,
    is_gpt4o_image,
)
from ailets.models.gpt4o.lib.tool_calls import ToolCalls


def rewrite_content_item(item: dict[str, Any]) -> ContentItem:
    if item["type"] == "text":
        assert is_content_item_text(item), "Content item must be a text"
        return item
    if item["type"] == "refusal":
        assert is_content_item_refusal(item), "Content item must be a refusal"
        return item

    assert is_gpt4o_image(item), "Content item must be an image"

    url = item["image_url"]["url"]
    assert url.startswith("data:"), "Image data-URL must start with 'data:'"
    assert "image/png" in url, "Only PNG images are supported"

    return ContentItemImage(type="image", url=url, content_type="image/png")


def _process_single_message(
    tool_calls: ToolCalls, gpt4o_message: Gpt4oMessage
) -> Optional[ChatMessage]:
    gpt4o_content = gpt4o_message.get("content")
    gpt4o_tool_calls = gpt4o_message.get("tool_calls")

    if gpt4o_content is None and gpt4o_tool_calls is None:
        raise ValueError("Response message has neither content nor tool_calls")

    if gpt4o_tool_calls:
        assert isinstance(gpt4o_tool_calls, list), "'tool_calls' must be a list"
        tool_calls.extend(gpt4o_tool_calls)

    if gpt4o_content:
        new_content: Content = []
        if isinstance(gpt4o_content, str):
            new_content = [
                ContentItemText(
                    type="text",
                    text=gpt4o_content,
                )
            ]
        else:
            assert isinstance(gpt4o_content, list), "Content must be a list"
            new_content = [rewrite_content_item(item) for item in gpt4o_content]

        message: ChatMessage = gpt4o_message.copy()  # type: ignore[assignment]
        message["content"] = new_content

        return message

    return None


async def response_to_messages(runtime: INodeRuntime) -> None:
    """Convert multiple responses to messages."""

    output = await runtime.open_write(None)

    messages: List[ChatMessage] = []
    sse_handler: Optional[SseHandler] = None
    tool_calls = ToolCalls()

    async for response in iter_streams_objects(
        runtime, None, sse_tokens=["data:", "[DONE]"]
    ):
        assert isinstance(response, dict), "Response must be a dictionary"

        if is_sse_object(response):
            if sse_handler is None:
                sse_handler = SseHandler(runtime, tool_calls, output)
            await sse_handler.handle_sse_object(response)
            continue

        assert "choices" in response, "Response must have 'choices' key"
        assert isinstance(response["choices"], list), "'choices' must be a list"

        for gpt4o_choice in response["choices"]:
            gpt4o_message = gpt4o_choice["message"]
            message = _process_single_message(tool_calls, gpt4o_message)
            if message is not None:
                messages.append(message)

    if len(messages) > 0:
        value = json.dumps(messages).encode("utf-8")
        await write_all(runtime, output, value)
    if sse_handler is not None:
        await sse_handler.done()

    tool_calls.to_dag(runtime)

    await runtime.close(output)
