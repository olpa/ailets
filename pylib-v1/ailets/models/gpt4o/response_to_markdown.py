import json
from ailets.cons.typing import Dependency, INodeRuntime


def _process_single_response(runtime: INodeRuntime, response: dict) -> str:
    """Process a single response and convert it to markdown.

    Args:
        runtime: The node runtime environment
        response: The response JSON from the API

    Returns:
        dict: The processed response as a JSON object
    """
    message = response["choices"][0]["message"]
    content = message.get("content")
    tool_calls = message.get("tool_calls")

    if content is None and tool_calls is None:
        raise ValueError("Response message has neither content nor tool_calls")

    if content is not None:
        return content

    #
    # Tool calls: call them and repeat the loop
    #

    dagops = runtime.dagops()
    loop_begin = dagops.get_upstream_node("gpt4o.messages_to_query")
    loop_begin = dagops.clone_node(loop_begin)

    runtime._env.print_dependency_tree(loop_begin)  # FIXME

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
    dagops.depend(loop_begin, [Dependency(source=idref_node)])

    #
    # Instantiate tools, run and connect them to the "chat history"
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

        runtime._env.print_dependency_tree(tool_msg_node_name)  # FIXME

        dagops.depend(loop_begin, [Dependency(source=tool_msg_node_name)])

    return ""


def response_to_markdown(runtime: INodeRuntime) -> None:
    """Convert multiple responses to markdown format."""

    results = []
    for i in range(runtime.n_of_streams(None)):
        response = json.loads(runtime.open_read(None, i).read())
        result = _process_single_response(runtime, response)
        if result:  # Only add non-empty results
            results.append(result)

    value = "\n\n".join(results)
    output = runtime.open_write(None)
    output.write(value)
    runtime.close_write(None)
