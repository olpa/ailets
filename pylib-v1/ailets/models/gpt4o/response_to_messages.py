from dataclasses import dataclass
import json
from typing import List, Optional, Sequence, Set
from ailets.cons.typing import (
    ChatMessageAssistant,
    ChatMessageContentImage,
    ChatMessageContentPlainText,
    ChatMessageContentText,
    ChatMessageStructuredContentItem,
    INodeRuntime,
)
from ailets.cons.util import (
    iter_streams_objects,
    write_all,
)
from ailets.models.gpt4o.typing import (
    Gpt4oChatMessageContent,
    Gpt4oChatMessageContentItem,
)


@dataclass
class InvalidationFlag:
    is_invalidated: bool
    fence: Optional[Set[str]] = None


def rewrite_content_item(
    runtime: INodeRuntime, item: Gpt4oChatMessageContentItem
) -> ChatMessageStructuredContentItem:
    if item["type"] == "text" or item["type"] == "refusal":
        return item
    assert item["type"] == "image_url", "Only text and image are supported"

    url = item["image_url"]["url"]
    assert url.startswith("data:"), "Image data-URL must start with 'data:'"
    assert "image/png" in url, "Only PNG images are supported"

    return ChatMessageContentImage(type="image", url=url, content_type="image/png")


def _process_single_message(
    runtime: INodeRuntime,
    response: dict,
    invalidation_flag_rw: InvalidationFlag,
) -> Optional[ChatMessageStructuredContentItem]:
    message = response["choices"][0]["message"]
    content: Gpt4oChatMessageContent = message.get("content")
    tool_calls = message.get("tool_calls")

    if content is None and tool_calls is None:
        raise ValueError("Response message has neither content nor tool_calls")

    if content:
        new_content: List[ChatMessageStructuredContentItem] = []
        if isinstance(content, ChatMessageContentPlainText):
            new_content = [
                ChatMessageContentText(
                    type="text",
                    text=content,
                )
            ]
        else:
            new_content = [rewrite_content_item(runtime, item) for item in content]

        message = message.copy()
        message["content"] = new_content

        return message

    assert tool_calls is not None, "tool_calls cannot be None at this point"

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
    idref_messages: Sequence[ChatMessageAssistant] = [message]
    idref_node = dagops.add_value_node(
        json.dumps(idref_messages).encode("utf-8"),
        explain='Feed "tool_calls" from output to input',
    )
    dagops.alias(".chat_messages", idref_node)

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


def response_to_messages(runtime: INodeRuntime) -> None:
    """Convert multiple responses to messages."""

    output = runtime.open_write(None)

    invalidation_flag = InvalidationFlag(is_invalidated=False)
    messages: List[ChatMessageStructuredContentItem] = []

    for response in iter_streams_objects(runtime, None):
        message = _process_single_message(runtime, response, invalidation_flag)
        if message is not None:
            messages.append(message)

    value = json.dumps(messages).encode("utf-8")
    write_all(runtime, output, value)
    runtime.close(output)
