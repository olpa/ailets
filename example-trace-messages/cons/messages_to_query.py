from typing import List, Dict


def messages_to_query(messages: List[Dict[str, str]]) -> str:
    """Convert chat messages into a query string."""
    return " ".join(msg["content"] for msg in messages)
