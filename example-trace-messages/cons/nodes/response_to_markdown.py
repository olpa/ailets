from cons.cons import Environment, Node


def response_to_markdown(response: dict, env: Environment, node: Node) -> str:
    """Convert the response to markdown format."""
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
        node_name = f"tool/{tool_name}/call"

        # Create node with tool call as fixed input
        def tool_call_result_to_chat_message(x):
            return x

        env.add_node(node_name, lambda _: tool_call_result_to_chat_message(tool_call))

        # Add as dependency of start node
        start_node.deps.append(node_name)

    # Connect end node to next in pipeline
    env.get_next_nodes(node)[0].deps.append(end_node.name)
    return ""
