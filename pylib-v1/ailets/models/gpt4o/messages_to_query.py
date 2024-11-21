import json
from typing import Sequence
from ailets.cons.typing import ChatMessage, INodeRuntime, ToolSpecification

url = "https://api.openai.com/v1/chat/completions"
method = "POST"
headers = {"Content-type": "application/json"}


def messages_to_query(runtime: INodeRuntime) -> None:
    """Convert chat messages into a query."""

    messages: list[ChatMessage] = []
    for i in range(runtime.n_of_streams(None)):
        stream = runtime.open_read(None, i)
        ith_messages: Sequence[ChatMessage] = json.loads(stream.read())
        messages.extend(ith_messages)

    tools = []
    for i in range(runtime.n_of_streams("toolspecs")):
        stream = runtime.open_read("toolspecs", i)
        toolspec: ToolSpecification = json.loads(stream.read())
        tools.append(
            {
                "type": "function",
                "function": toolspec,
            }
        )
    tools_param = {"tools": tools} if tools else {}

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
        "body": {
            "model": "gpt-4o-mini",
            "messages": messages,
            **tools_param,
        },
    }

    output = runtime.open_write(None)
    output.write(json.dumps(value))
    runtime.close_write(None)
