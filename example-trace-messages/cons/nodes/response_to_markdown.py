def response_to_markdown(response: dict) -> str:
    """Convert the response to markdown format."""
    content = response["choices"][0]["message"]["content"]
    return content
