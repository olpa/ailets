import json
from ailets.cons.typing import INodeRuntime

url = "https://api.openai.com/v1/images/generations"
method = "POST"
headers = {"Content-type": "application/json"}


def messages_to_query(runtime: INodeRuntime) -> None:
    """Convert prompt message into a DALL-E query."""

    messages = []
    for i in range(runtime.n_of_streams(None)):
        stream = runtime.open_read(None, i)
        messages.extend(json.loads(stream.read()))

    # Get the last user message as the prompt
    prompt = None
    for message in reversed(messages):
        if message.get("role") == "user":
            prompt = message.get("content")
            break

    if not prompt:
        raise ValueError("No user prompt found in messages")

    creds = {}
    for i in range(runtime.n_of_streams("credentials")):
        stream = runtime.open_read("credentials", i)
        creds.update(json.loads(stream.read()))

    value = {
        "url": url,
        "method": method,
        "headers": {
            **headers,
            **creds,
        },
        "body": {"model": "dall-e-3", "prompt": prompt, "n": 1, "size": "1024x1024"},
    }

    output = runtime.open_write(None)
    output.write(json.dumps(value))
    runtime.close_write(None)
