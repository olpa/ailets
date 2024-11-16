import json
from ...typing import INodeRuntime


def toolcall_to_messages(runtime: INodeRuntime) -> None:
    """Convert tool result and spec into a chat message.

    Reads:
        Stream None: Tool result
        Stream "llmspec": LLM specification

    Writes:
        A single message in OpenAI chat format
    """
    # Read inputs
    n_tool_results = runtime.n_of_streams(None)
    assert n_tool_results == 1, "Expected exactly one tool result"
    n_specs = runtime.n_of_streams("llmspec")
    assert n_specs == 1, "Expected exactly one tool spec"

    tool_result = runtime.open_read(None, 0).read()
    spec = json.loads(runtime.open_read("llmspec", 0).read())

    # Extract function details from spec
    function_name = spec["function"]["name"]
    tool_call_id = spec["id"]

    # Parse arguments string to dict
    try:
        arguments = json.loads(spec["function"]["arguments"])
    except json.JSONDecodeError as e:
        print(f"Failed to parse tool arguments as JSON: {str(e)}")
        raise

    # Construct response content
    content = {
        **arguments,  # Include original parameters
        function_name: tool_result,  # Add tool result under function name
    }

    # Write output
    output = runtime.open_write(None)
    output.write(
        json.dumps(
            [
                {
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": json.dumps(content),
                }
            ]
        )
    )
    runtime.close_write(None)
