def credentials(_: list[str]) -> dict:
    """Return API credentials as a dictionary."""
    return {
        "Authorization": "Bearer ##OPENAI_API_KEY##",
        # "OpenAI-Organization": "",
    }
