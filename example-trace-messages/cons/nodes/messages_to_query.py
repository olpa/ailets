from typing import List, Dict, Any, Optional

url = "https://api.openai.com/v1/chat/completions"
method = "POST"
headers = {"Content-type": "application/json"}


def messages_to_query(
    messages: List[List[Dict[str, str]]],
    credentials: List[Dict[str, str]],
    toolspecs: Optional[List[str]] = None,
    toolcalls: Optional[List[str]] = None,
) -> Dict[str, Any]:
    """Convert chat messages into a query."""
    print("TODO: toolcalls:", toolcalls)  # FIXME

    body = {
        "model": "gpt-4o-mini",
        "messages": [msg for msgs in messages for msg in msgs],
    }

    if toolspecs is not None:
        formatted_tools = [{"type": "function", "function": tool} for tool in toolspecs]
        body["tools"] = formatted_tools

    return {
        "url": url,
        "method": method,
        "headers": {
            **headers,
            **{k: v for cred in credentials for k, v in cred.items()},
        },
        "body": body,
    }
