import json
from ..cons.atyping import ChatMessageTool, ContentItemFunction, INodeRuntime
from ..cons.util import read_all, write_all


async def toolcall_to_messages(runtime: INodeRuntime) -> None:
    """Convert tool result and spec into a chat message.

    Reads:
        Stream None: Tool result
        Stream "llm_tool_spec": LLM specification

    Writes:
        A single message in OpenAI chat format
    """
    n_tool_results = runtime.n_of_streams("")
    assert (
        n_tool_results == 1
    ), f"Expected exactly one tool result, got {n_tool_results}"
    n_specs = runtime.n_of_streams("llm_tool_spec")
    assert n_specs == 1, f"Expected exactly one tool spec, got {n_specs}"

    fd = await runtime.open_read("", 0)
    tool_result = (await read_all(runtime, fd)).decode("utf-8")
    await runtime.close(fd)

    fd = await runtime.open_read("llm_tool_spec", 0)
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

    fd = await runtime.open_write("")
    value = json.dumps([chat_message]).encode("utf-8")
    await write_all(runtime, fd, value)
    await runtime.close(fd)
