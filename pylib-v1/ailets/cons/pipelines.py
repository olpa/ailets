from .cons import Environment, Node
from .nodes.prompt_to_messages import prompt_to_messages
from .nodes.messages_to_query import messages_to_query
from .nodes.query import query
from .nodes.response_to_markdown import response_to_markdown
from .nodes.stdout import stdout
from .nodes.credentials import credentials
from .nodes.tool_get_user_name import get_spec_for_get_user_name, run_get_user_name
from .nodes.toolcall_to_messages import toolcall_to_messages


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
    env: Environment, initial_prompt: str = "hello", tools: list[str] = []
) -> Node:
    """Create a chain of nodes that process a prompt into markdown."""
    # Define nodes and their dependencies
    node_v = env.add_value_node(initial_prompt, explain="Initial prompt")
    node_ptm = env.add_node("prompt_to_messages", prompt_to_messages, [node_v.name])
    node_creds = env.add_node("credentials", credentials)

    # Get tool spec nodes from tool names
    tool_specs = [must_get_tool_spec(env, tool_name) for tool_name in tools]

    node_mtq = env.add_node(
        "messages_to_query",
        messages_to_query,
        [
            node_ptm.name,
            (node_creds.name, "credentials"),
            *[(tool_spec.name, "toolspecs") for tool_spec in tool_specs],
        ],
    )
    node_q = env.add_node("query", query, [node_mtq.name])
    node_rtm = env.add_node("response_to_markdown", response_to_markdown, [node_q.name])
    final_node = env.add_node("stdout", stdout, [node_rtm.name])

    return final_node
