from typing import List, Dict


def prompt_to_messages(
    prompt: List[str], params: List[Dict[str, any]] = None
) -> List[Dict[str, str]]:
    """Convert a list of prompts into a list of chat messages."""
    return [{"role": "user", "content": p} for p in prompt]
