from .cons import Environment, Node
from .nodes.prompt_to_messages import prompt_to_messages
from .nodes.messages_to_query import messages_to_query
from .nodes.query import query
from .nodes.response_to_markdown import response_to_markdown
from .nodes.stdout import stdout
from .nodes.credentials import credentials


def prompt_to_md(env: Environment, initial_prompt: str = "hello") -> Node:
    """Create a chain of nodes that process a prompt into markdown."""
    # Define nodes and their dependencies
    env.add_node("initial_prompt", lambda: initial_prompt)
    env.add_node("prompt_to_messages", prompt_to_messages, ["initial_prompt"])
    env.add_node("credentials", credentials)
    env.add_node(
        "messages_to_query", messages_to_query, ["prompt_to_messages", "credentials"]
    )
    env.add_node("query", query, ["messages_to_query"])
    env.add_node("response_to_markdown", response_to_markdown, ["query"])
    env.add_node("stdout", stdout, ["response_to_markdown"])

    # Return the final node
    return env.get_node("response_to_markdown")