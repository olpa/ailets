import json
from ..cons.atyping import (
    ChatMessageTool,
    ContentItemFunction,
    INodeRuntime,
    StdHandles,
)
from ..cons.util import write_all
from ..cons.input_reader import read_all


async def toolcall_to_messages(runtime: INodeRuntime) -> None:
    """Convert tool result and spec into a chat message.

    Reads:
        Input slot "": Tool result
        Input slot "llm_tool_spec": LLM specification

    Writes:
        A single message in OpenAI chat format
    """
    tool_result = (await read_all(runtime, StdHandles.stdin)).decode("utf-8")

    fd = await runtime.open_read("llm_tool_spec")
    spec: ContentItemFunction = json.loads(
        (await read_all(runtime, fd)).decode("utf-8")
    )
    await runtime.close(fd)

    #
    # LLM tool call spec
    #
    # ```
    # {
    #     "id": "call-62136354",
    #     "function": {
    #         "arguments": "{\"order_id\": \"order_12345\"}",
    #         "name": "get_delivery_date",
    #     },
    #     "type": "function"
    # }
    # ```
    #
    function_name = spec["function"]["name"]
    tool_call_id = spec["id"]

    try:
        arguments = json.loads(spec["function"]["arguments"])
    except json.JSONDecodeError as e:
        print(f"Failed to parse tool arguments as JSON: {str(e)}")
        raise

    # Construct response content
    # Note that the argument list is extended with the item
    # `function_name: tool_result`
    #
    # ```
    # {
    #     "role": "tool",
    #     "content": "{\"order_id\": \"order_12345\",
    #                  \"get_delivery_date\": \"2024-01-01\"}",
    #     "tool_call_id": "call-62136354"
    # }
    # ```
    #
    content = {
        **arguments,
        function_name: tool_result,
    }

    chat_message: ChatMessageTool = {
        "role": "tool",
        "content": [
            {
                "type": "text",
                "text": json.dumps(content),
            },
        ],
        "tool_call_id": tool_call_id,
    }

    value = json.dumps([chat_message]).encode("utf-8")
    await write_all(runtime, StdHandles.stdout, value)
