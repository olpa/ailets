import json
from typing import Sequence
from ailets.cons.typing import ChatMessage, INodeRuntime, ToolSpecification
from ailets.cons.util import read_all, write_all

url = "https://api.openai.com/v1/chat/completions"
method = "POST"
headers = {
    "Content-type": "application/json",
    "Authorization": "Bearer {{secret('openai','gpt4o')}}",
}


def messages_to_query(runtime: INodeRuntime) -> None:
    """Convert chat messages into a query."""

    messages: list[ChatMessage] = []
    for i in range(runtime.n_of_streams(None)):
        fd = runtime.open_read(None, i)
        content = read_all(runtime, fd).decode("utf-8")
        ith_messages: Sequence[ChatMessage] = json.loads(content)
        runtime.close(fd)
        messages.extend(ith_messages)

    tools = []
    for i in range(runtime.n_of_streams("toolspecs")):
        fd = runtime.open_read("toolspecs", i)
        content = read_all(runtime, fd).decode("utf-8")
        toolspec: ToolSpecification = json.loads(content)
        runtime.close(fd)
        tools.append(
            {
                "type": "function",
                "function": toolspec,
            }
        )
    tools_param = {"tools": tools} if tools else {}

    value = {
        "url": url,
        "method": method,
        "headers": headers,
        "body": {
            "model": "gpt-4o-mini",
            "messages": messages,
            **tools_param,
        },
    }

    fd = runtime.open_write(None)
    write_all(runtime, fd, json.dumps(value).encode("utf-8"))
    runtime.close(fd)
