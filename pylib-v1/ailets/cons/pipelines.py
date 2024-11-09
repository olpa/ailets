from .cons import Environment, Node
from .nodes.prompt_to_messages import prompt_to_messages
from .nodes.messages_to_query import messages_to_query
from .nodes.query import query
from .nodes.response_to_markdown import response_to_markdown
from .nodes.stdout import stdout
from .nodes.credentials import credentials
from .nodes.tool_get_user_name import get_spec_for_get_user_name, run_get_user_name
from .nodes.toolcall_to_messages import toolcall_to_messages
from typing import Union, List, Tuple


def get_func_map():
    """Create mapping of node names to their functions."""
    return {
        "value": lambda _, node: node.cache,
        "prompt_to_messages": prompt_to_messages,
        "credentials": credentials,
        "messages_to_query": messages_to_query,
        "query": query,
        "response_to_markdown": response_to_markdown,
        "stdout": stdout,
        "tool/get_user_name/spec": get_spec_for_get_user_name,
        "tool/get_user_name/call": run_get_user_name,
        "toolcall_to_messages": toolcall_to_messages,
    }


def must_get_tool_spec(env: Environment, tool_name: str) -> Node:
    node_name = f"tool/{tool_name}/spec"
    (tool_spec_func, _) = env.get_tool(tool_name)
    return env.add_node(node_name, tool_spec_func)


def prompt_to_md(
    env: Environment,
    prompt: List[Union[str, Tuple[str, str]]] = ["Hello!"],
    tools: list[str] = [],
) -> Node:
    """Create a chain of nodes that process prompts into markdown.

    Args:
        env: The environment
        prompts: List of prompts. Each prompt can be either:
                - str: treated as a regular dependency
                - tuple[str, str]: (text, type) where type creates a named dependency
        tools: List of tool names to use

    Returns:
        The final node in the chain
    """
    # Create nodes for each prompt item
    nodes_ptm = []
    for prompt_item in prompt:
        if isinstance(prompt_item, str):
            prompt_text = prompt_item
            type_deps = []
        else:
            prompt_text, prompt_type = prompt_item
            node_type = env.add_value_node({"type": prompt_type}, explain="Prompt type")
            type_deps = [(node_type.name, "type")]

        node_v = env.add_value_node(prompt_text, explain="Prompt")
        node_ptm = env.add_node(
            "prompt_to_messages", prompt_to_messages, [node_v.name] + type_deps
        )
        nodes_ptm.append(node_ptm.name)

    # Get tool spec nodes
    tool_specs = [must_get_tool_spec(env, tool_name) for tool_name in tools]

    # Create credentials node
    node_creds = env.add_node("credentials", credentials)

    # Combine all prompts and tools in messages_to_query
    node_mtq = env.add_node(
        "messages_to_query",
        messages_to_query,
        [
            *nodes_ptm,
            (node_creds.name, "credentials"),
            *[(spec.name, "toolspecs") for spec in tool_specs],
        ],
    )

    # Rest of the pipeline remains the same
    node_q = env.add_node("query", query, [node_mtq.name])
    node_rtm = env.add_node("response_to_markdown", response_to_markdown, [node_q.name])
    node_out = env.add_node("stdout", stdout, [node_rtm.name])

    return node_out
