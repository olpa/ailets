from typing import List, Dict, Tuple, Sequence, Any


def prompt_to_messages(prompt: Sequence[Tuple[str, str]]) -> List[Dict[str, Any]]:
    """Convert a list of prompts into a list of chat messages."""

    def to_llm_item(item: Tuple[str, str]) -> Dict[str, Any]:
        content, content_type = item
        if content_type == "text":
            return {"type": "text", "text": content}
        if content_type == "image_url":
            return {"type": "image_url", "image_url": {"url": content}}
        raise ValueError(f"Unknown content type: {content_type}")

    return [
        {
            "role": "user",
            "content": [to_llm_item(p) for p in prompt],
        }
    ]
