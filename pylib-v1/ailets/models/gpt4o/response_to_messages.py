from dataclasses import dataclass
import json
from typing import Any, List, Optional, Set
from ailets.cons.typeguards import (
    is_content_item_refusal,
    is_content_item_text,
)
from ailets.cons.atyping import (
    ChatMessage,
    Content,
    ContentItem,
    ContentItemFunction,
    ContentItemImage,
    ContentItemText,
    INodeRuntime,
)
from ailets.cons.util import (
    iter_streams_objects,
    write_all,
)
from ailets.models.gpt4o.sse import SseHandler, is_sse_object
from ailets.models.gpt4o.typing import (
    Gpt4oMessage,
    is_gpt4o_image,
)


@dataclass
class InvalidationFlag:
    is_invalidated: bool
    fence: Optional[Set[str]] = None


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
    runtime: INodeRuntime,
    gpt4o_message: Gpt4oMessage,
    invalidation_flag_rw: InvalidationFlag,
) -> Optional[ChatMessage]:
    gpt4o_content = gpt4o_message.get("content")
    gpt4o_tool_calls = gpt4o_message.get("tool_calls")

    if gpt4o_content is None and gpt4o_tool_calls is None:
        raise ValueError("Response message has neither content nor tool_calls")

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

    assert gpt4o_tool_calls is not None, "tool_calls cannot be None at this point"
    assert isinstance(gpt4o_tool_calls, list), "'tool_calls' must be a list"

    #
    # Tool calls
    #

    dagops = runtime.dagops()
    if not invalidation_flag_rw.is_invalidated:
        invalidation_flag_rw.is_invalidated = True
        dagops.detach_from_alias(".chat_messages")
    #
    # Put "tool_calls" to the "chat history"
    #
    tool_calls: List[ContentItemFunction] = []
    for gpt4o_tool_call in gpt4o_tool_calls:
        tool_call: ContentItemFunction = gpt4o_tool_call
        tool_calls.append(tool_call)

    tool_calls_message: ChatMessage = gpt4o_message.copy()  # type: ignore[assignment]
    tool_calls_message["content"] = tool_calls
    tool_calls_node = dagops.add_value_node(
        json.dumps([tool_calls_message]).encode("utf-8"),
        explain='Feed "tool_calls" from output to input',
    )
    dagops.alias(".chat_messages", tool_calls_node)

    #
    # Instantiate tools and connect them to the "chat history"
    #
    for tool_call in tool_calls:
        tool_spec_node_name = dagops.add_value_node(
            json.dumps(tool_call).encode("utf-8"),
            explain="Tool call spec from llm",
        )

        tool_name = tool_call["function"]["name"]
        tool_final_node_name = dagops.instantiate_with_deps(
            f".tool.{tool_name}", {".tool_input": tool_spec_node_name}
        )

        tool_msg_node_name = dagops.instantiate_with_deps(
            ".toolcall_to_messages",
            {
                ".llm_tool_spec": tool_spec_node_name,
                ".tool_output": tool_final_node_name,
            },
        )
        dagops.alias(".chat_messages", tool_msg_node_name)
    #
    # Re-run the model
    #
    rerun_node_name = dagops.instantiate_with_deps(".gpt4o", {})
    dagops.alias(".model_output", rerun_node_name)

    return None


async def response_to_messages(runtime: INodeRuntime) -> None:
    """Convert multiple responses to messages."""

    output = await runtime.open_write(None)

    invalidation_flag = InvalidationFlag(is_invalidated=False)
    messages: List[ChatMessage] = []
    sse_handler: Optional[SseHandler] = None

    async for response in iter_streams_objects(
        runtime, None, sse_tokens=["data:", "[DONE]"]
    ):
        assert isinstance(response, dict), "Response must be a dictionary"

        if is_sse_object(response):
            if sse_handler is None:
                sse_handler = SseHandler(runtime, output)
            await sse_handler.handle_sse_object(response)
            continue

        assert "choices" in response, "Response must have 'choices' key"
        assert isinstance(response["choices"], list), "'choices' must be a list"

        for gpt4o_choice in response["choices"]:
            gpt4o_message = gpt4o_choice["message"]
            message = _process_single_message(runtime, gpt4o_message, invalidation_flag)
            if message is not None:
                messages.append(message)

    if len(messages) > 0:
        value = json.dumps(messages).encode("utf-8")
        await write_all(runtime, output, value)
    if sse_handler is not None:
        await sse_handler.done()

    await runtime.close(output)
