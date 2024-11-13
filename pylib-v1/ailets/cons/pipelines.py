from .cons import Dependency, Environment, Node
from .nodes.prompt_to_messages import prompt_to_messages
from .nodes.messages_to_query import messages_to_query
from .nodes.query import query
from .nodes.response_to_markdown import response_to_markdown
from .nodes.stdout import stdout
from .nodes.credentials import credentials
from .nodes.tool_get_user_name import get_spec_for_get_user_name, run_get_user_name
from .nodes.toolcall_to_messages import toolcall_to_messages
from typing import Union, Tuple, Sequence


def get_func_map():
    """Create mapping of node names to their functions."""
    return {
        "typed_value": lambda _: None,
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
    prompt: Sequence[Union[str, Tuple[str, str]]] = ["Hello!"],
    tools: Sequence[str] = [],
) -> Node:
    """Create a chain of nodes that process prompts into markdown.

    Args:
        env: The environment
        prompts: Sequence of prompts. Each prompt can be either:
                - str: treated as a regular message
                - tuple[str, str]: (text, type) for typed messages
        tools: Sequence of tool names to use

    Returns:
        The final node in the chain
    """

    # Create nodes for each prompt item
    def prompt_to_node(prompt_item: Union[str, Tuple[str, str]]) -> str:
        if isinstance(prompt_item, str):
            prompt_text = prompt_item
            prompt_type = "text"
        else:
            prompt_text, prompt_type = prompt_item
        node_tv = env.add_typed_value_node(prompt_text, prompt_type, explain="Prompt")
        return node_tv.name

    nodes_tvs: Sequence[str] = [prompt_to_node(prompt_item) for prompt_item in prompt]

    node_ptm = env.add_node(
        "prompt_to_messages",
        prompt_to_messages,
        [
            Dependency(dep_name=None, node_name=node_tv, stream_name=None)
            for node_tv in nodes_tvs
        ]
        + [
            Dependency(dep_name="type", node_name=node_tv, stream_name="type")
            for node_tv in nodes_tvs
        ],
    )

    # Get tool spec nodes
    tool_specs = [must_get_tool_spec(env, tool_name) for tool_name in tools]

    # Create credentials node
    node_creds = env.add_node("credentials", credentials)

    # Combine all prompts and tools in messages_to_query
    node_mtq = env.add_node(
        "messages_to_query",
        messages_to_query,
        [
            Dependency(dep_name=None, node_name=node_ptm.name, stream_name=None),
            Dependency(
                dep_name="credentials", node_name=node_creds.name, stream_name=None
            ),
            *[
                Dependency(dep_name="toolspecs", node_name=spec.name, stream_name=None)
                for spec in tool_specs
            ],
        ],
    )

    # Rest of the pipeline remains the same
    node_q = env.add_node(
        "query",
        query,
        [Dependency(dep_name=None, node_name=node_mtq.name, stream_name=None)],
    )
    node_rtm = env.add_node(
        "response_to_markdown",
        response_to_markdown,
        [Dependency(dep_name=None, node_name=node_q.name, stream_name=None)],
    )
    node_out = env.add_node(
        "stdout",
        stdout,
        [Dependency(dep_name=None, node_name=node_rtm.name, stream_name=None)],
    )

    return node_out
