from typing import List, Dict
from cons.cons import Environment, Node


def prompt_to_messages(prompt: str) -> List[Dict[str, str]]:
    """Convert a prompt into a list of chat messages."""
    return [{"role": "user", "content": prompt}]


def messages_to_query(messages: List[Dict[str, str]]) -> str:
    """Convert chat messages into a query string."""
    return " ".join(msg["content"] for msg in messages)


def query(query_str: str) -> str:
    """Perform the actual query (placeholder)."""
    return f"Response to: {query_str}"


def response_to_markdown(response: str) -> str:
    """Convert the response to markdown format."""
    return f"# Response\n\n{response}"


def prompt_to_md(env: Environment, initial_prompt: str = "hello") -> Node:
    """Create a chain of nodes that process a prompt into markdown."""
    # Define nodes and their dependencies
    env.add_node("initial_prompt", lambda: initial_prompt)
    env.add_node("prompt_to_messages", prompt_to_messages, {"initial_prompt"})
    env.add_node("messages_to_query", messages_to_query, {"prompt_to_messages"})
    env.add_node("query", query, {"messages_to_query"})
    env.add_node("response_to_markdown", response_to_markdown, {"query"})

    # Return the final node
    return env.get_node("response_to_markdown")
