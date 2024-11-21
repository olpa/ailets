from dataclasses import dataclass
import json
from typing import List, Optional, Sequence, Set
from ailets.cons.typing import (
    ChatMessage,
    ChatMessageAssistant,
    INodeRuntime,
)


@dataclass
class InvalidationFlag:
    is_invalidated: bool
    fence: Optional[Set[str]] = None


def _process_single_message(
    runtime: INodeRuntime,
    response: dict,
    invalidation_flag_rw: InvalidationFlag,
) -> Optional[ChatMessage]:
    message = response["choices"][0]["message"]
    content = message.get("content")
    tool_calls = message.get("tool_calls")

    if content is None and tool_calls is None:
        raise ValueError("Response message has neither content nor tool_calls")
    if content is not None:
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
    idref_node = dagops.add_typed_value_node(
        json.dumps(idref_messages),
        "",
        explain='Feed "tool_calls" from output to input',
    )
    dagops.alias(".chat_messages", idref_node)

    #
    # Instantiate tools and connect them to the "chat history"
    #
    for tool_call in tool_calls:
        tool_spec_node_name = dagops.add_typed_value_node(
            json.dumps(tool_call), "", explain="Tool call spec from llm"
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
    messages: List[ChatMessage] = []

    for i in range(runtime.n_of_streams(None)):
        response = json.loads(runtime.open_read(None, i).read())
        message = _process_single_message(runtime, response, invalidation_flag)
        if message is not None:
            messages.append(message)

    json.dump(messages, output)
    runtime.close_write(None)
