import json
from ailets.cons.typing import ChatMessage, INodeRuntime
from ailets.cons.util import iter_streams_objects, write_all

url = "https://api.openai.com/v1/chat/completions"
method = "POST"
headers = {
    "Content-type": "application/json",
    "Authorization": "Bearer {{secret('openai','gpt4o')}}",
}


def messages_to_query(runtime: INodeRuntime) -> None:
    """Convert chat messages into a query."""

    messages: list[ChatMessage] = list(
        iter_streams_objects(runtime, None)  # type: ignore[arg-type]
    )

    tools = []
    for toolspec in iter_streams_objects(runtime, "toolspecs"):
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
