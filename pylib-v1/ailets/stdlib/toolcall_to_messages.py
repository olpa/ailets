import json
from ailets.atyping import (
    ContentItemCtl,
    ContentItemFunction,
    ContentItemText,
    INodeRuntime,
    StdHandles,
)
from ailets.cons.util import write_all
from ailets.io.input_reader import read_all


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
    # Tool call message
    #
    # ```
    # [{
    #     "type": "function"
    #     "id": "call-62136354",
    #     "name": "get_delivery_date"
    #   },{
    #     "arguments": "{\"order_id\": \"order_12345\"}"
    # }]
    # ```
    #
    assert isinstance(spec, list), "Tool call message must be a list"
    assert len(spec) == 2, "Tool call message must have exactly two items"
    spec0 = spec[0]
    assert isinstance(spec0, dict), "Tool call attributes must be a dict"
    spec1 = spec[1]
    assert isinstance(spec1, str), "Tool call arguments must be a string"
    assert "name" in spec0, "Tool call attributes must have a name"
    assert "id" in spec0, "Tool call attributes must have an id"
    assert "arguments" in spec1, "Tool call arguments must be a string"
    function_name = spec0["name"]
    tool_call_id = spec0["id"]
    arguments = spec1["arguments"]

    # Old comment:
    #
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
    # New comment:
    #
    # As an experiment, we go away from the official OpenAI example,
    # and instead of patching arguments, we add a new item to the content.
    #
    tool_marker: ContentItemCtl = [
        {"type": "ctl"},
        {
            "role": "tool",
            "tool_call_id": tool_call_id,
        },
    ]
    await write_all(runtime, StdHandles.stdout, json.dumps(tool_marker).encode("utf-8"))

    repeat_args: ContentItemText = [{"type": "text"}, {"text": arguments}]
    await write_all(runtime, StdHandles.stdout, json.dumps(repeat_args).encode("utf-8"))

    result_text: ContentItemText = [
        {"type": "text"},
        {"text": json.dumps({function_name: tool_result})},
    ]
    await write_all(runtime, StdHandles.stdout, json.dumps(result_text).encode("utf-8"))
