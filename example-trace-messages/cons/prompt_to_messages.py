from typing import List, Dict


def prompt_to_messages(prompt: str) -> List[Dict[str, str]]:
    """Convert a prompt into a list of chat messages."""
    return [{"role": "user", "content": prompt}]
