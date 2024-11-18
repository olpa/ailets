import json
from ailets.cons.typing import INodeRuntime


def _process_single_response(runtime: INodeRuntime, response: dict) -> str:
    message = response["choices"][0]["message"]
    content = message.get("content")
    tool_calls = message.get("tool_calls")

    if content is None and tool_calls is None:
        raise ValueError("Response message has neither content nor tool_calls")

    if content is not None:
        return content

    #
    # Tool calls
    #

    dagops = runtime.dagops()

    #
    # Put "tool_calls" to the "chat history"
    #
    idref_messages = [
        {
            "role": message["role"],
            "tool_calls": tool_calls,
        }
    ]
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
        tool_final_node_name = dagops.instantiate_tool(tool_name, tool_spec_node_name)

        tool_msg_node_name = dagops.instantiate_with_deps(
            ".toolcall_to_messages",
            {
                ".llm_tool_spec": tool_spec_node_name,
                ".tool_output": tool_final_node_name,
            },
        )
        dagops.alias(".chat_messages", tool_msg_node_name)

    return ""


def response_to_markdown(runtime: INodeRuntime) -> None:
    """Convert multiple responses to markdown format."""

    output = runtime.open_write(None)

    dagops = runtime.dagops()
    old_chat_messages = dagops.expand_alias(".chat_messages")

    for i in range(runtime.n_of_streams(None)):
        response = json.loads(runtime.open_read(None, i).read())
        result = _process_single_response(runtime, response)
        if result:  # Only write non-empty results
            if i > 0:
                output.write("\n\n")
            output.write(result)

    new_chat_messages = dagops.expand_alias(".chat_messages")
    if len(new_chat_messages) > len(old_chat_messages):
        dagops.invalidate(".chat_messages", old_chat_messages)

    runtime.close_write(None)
