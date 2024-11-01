from typing import List, Dict, Any

url = "https://api.openai.com/v1/chat/completions"
method = "POST"
headers = {"Content-type": "application/json"}


def messages_to_query(
    messages: List[Dict[str, str]], credentials: Dict[str, str], tools: List[str]
) -> Dict[str, Any]:
    """Convert chat messages into a query."""
    formatted_tools = [{"type": "function", "function": tool} for tool in tools]
    return {
        "url": url,
        "method": method,
        "headers": {**headers, **credentials},
        "body": {
            "model": "gpt-4o-mini",
            "messages": messages,
            "tools": formatted_tools,
        },
    }
