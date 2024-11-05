from cons.cons import Environment, Node


def response_to_markdown(response: list[dict], env: Environment, node: Node) -> str:
    """Convert the response to markdown format."""
    assert len(response) == 1, "Expected exactly one response"
    response = response[0]

    message = response["choices"][0]["message"]
    content = message.get("content")
    tool_calls = message.get("tool_calls")

    if content is None and tool_calls is None:
        raise ValueError("Response message has neither content nor tool_calls")
    if content is not None:
        return content

    pipeline = env.clone_path("messages_to_query", node.name)
    start_node = pipeline[0]
    end_node = pipeline[-1]

    for tool_call in tool_calls:
        tool_name = tool_call["function"]["name"]
        short_node_name = f"tool/{tool_name}/call"

        # Create node with tool call as fixed input
        def tool_call_result_to_chat_message(x):
            return x

        tool_node = env.add_node(
            short_node_name, lambda _: tool_call_result_to_chat_message(tool_call)
        )

        start_node.deps.append((tool_node.name, "toolcalls"))

    # Connect end node to next in pipeline
    for next_node in env.get_next_nodes(node):
        next_node.deps.append((end_node.name, None))

    return ""
