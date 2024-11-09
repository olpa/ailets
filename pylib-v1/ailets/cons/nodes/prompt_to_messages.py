from typing import List, Dict, Any, Union, Tuple


def prompt_to_messages(
    prompts: List[Union[str, Tuple[str, str]]], params: List[Dict[str, Any]] = None
) -> List[Dict[str, str]]:
    """Convert a list of prompts into a list of chat messages."""
    return [{"role": "user", "content": p} for p in prompts]
