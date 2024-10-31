from cons.cons import Environment, Node
from cons.prompt_to_messages import prompt_to_messages
from cons.messages_to_query import messages_to_query
from cons.query import query
from cons.response_to_markdown import response_to_markdown
from cons.stdout import stdout


def prompt_to_md(env: Environment, initial_prompt: str = "hello") -> Node:
    """Create a chain of nodes that process a prompt into markdown."""
    # Define nodes and their dependencies
    env.add_node("initial_prompt", lambda: initial_prompt)
    env.add_node("prompt_to_messages", prompt_to_messages, {"initial_prompt"})
    env.add_node("messages_to_query", messages_to_query, {"prompt_to_messages"})
    env.add_node("query", query, {"messages_to_query"})
    env.add_node("response_to_markdown", response_to_markdown, {"query"})
    env.add_node("stdout", stdout, {"response_to_markdown"})

    # Return the final node
    return env.get_node("response_to_markdown")
