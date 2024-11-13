import json
from ailets.cons.typing import INodeRuntime


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

    """
    pipeline = env.clone_path("messages_to_query", node.name)
    start_node = pipeline[0]
    end_node = pipeline[-1]

        idref_node = env.add_typed_value_node(
        json.dumps(idref_messages),
        "",
        explain='Feed "tool_calls" from output to input',
    )
    start_node.deps.append(
        Dependency(dep_name=None, node_name=idref_node.name, stream_name=None)
    )

    for tool_call in tool_calls:
        tool_name = tool_call["function"]["name"]
        short_node_name = f"tool/{tool_name}/call"
        (_, tool_func) = env.get_tool(tool_name)

        tool_spec_node = env.add_node(
            "value", lambda _: tool_call, explain="Tool call spec from llm"
        )
        tool_call_node = env.add_node(
            short_node_name,
            tool_func,
            [
                Dependency(
                    dep_name=None, node_name=tool_spec_node.name, stream_name=None
                )
            ],
        )
        tool_msg_node = env.add_node(
            "toolcall_to_messages",
            toolcall_to_messages,
            [
                Dependency(
                    dep_name=None, node_name=tool_call_node.name, stream_name=None
                ),
                Dependency(
                    dep_name="llm_spec", node_name=tool_spec_node.name, stream_name=None
                ),
            ],
        )

        start_node.deps.append(
            Dependency(dep_name=None, node_name=tool_msg_node.name, stream_name=None)
        )
    # Connect end node to next in pipeline
    for next_node in env.get_next_nodes(node):
        next_node.deps.append(
            Dependency(dep_name=None, node_name=end_node.name, stream_name=None)
        )

    return ""


    """

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
    output.write(json.dumps(value))
    runtime.close_write(None)
