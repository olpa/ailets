from dataclasses import dataclass
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


@dataclass(frozen=True)
class NodeDesc:
    name: str
    inputs: Sequence[Dependency]


prompt_to_messages_desc = NodeDesc(
    name="prompt_to_messages",
    inputs=[
        Dependency(source="prompt"),
        Dependency(name="type", source="prompt", stream="type"),
    ],
)
credentials_desc = NodeDesc(
    name="credentials",
    inputs=[],
)

messages_to_query_desc = NodeDesc(
    name="messages_to_query",
    inputs=[
        Dependency(source="prompt_to_messages"),
        Dependency(name="credentials", source="credentials", stream="credentials"),
    ],
)

query_desc = NodeDesc(
    name="query",
    inputs=[
        Dependency(source="messages_to_query"),
    ],
)

response_to_markdown_desc = NodeDesc(
    name="response_to_markdown",
    inputs=[
        Dependency(source="query"),
    ],
)

stdout_desc = NodeDesc(
    name="stdout",
    inputs=[
        Dependency(source="response_to_markdown"),
    ],
)

tool_get_user_name_desc = NodeDesc(
    name="tool/get_user_name",
    inputs=[],
)


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
) -> None:
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
    def prompt_to_node(prompt_item: Union[str, Tuple[str, str]]) -> None:
        if isinstance(prompt_item, str):
            prompt_text = prompt_item
            prompt_type = "text"
        else:
            prompt_text, prompt_type = prompt_item
        node_tv = env.add_typed_value_node(prompt_text, prompt_type, explain="Prompt")
        env.alias("prompt", node_tv.name)

    for prompt_item in prompt:
        prompt_to_node(prompt_item)

    for node_desc in [
        prompt_to_messages_desc,
        credentials_desc,
        messages_to_query_desc,
        query_desc,
        response_to_markdown_desc,
        stdout_desc,
    ]:
        node = env.add_node(
            node_desc.name, get_func_map()[node_desc.name], node_desc.inputs
        )
        env.alias(node_desc.name, node.name)

    # TODO: Add tool spec nodes

    # TODO: validate that all the deps are valid
