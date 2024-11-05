import json
from typing import Any


def toolcall_to_message(inputs: list[Any], llm_spec: list[dict]) -> dict[str, str]:
    """Convert tool result and spec into a chat message."""
    assert len(inputs) == 1, "Expected exactly one tool result"
    assert len(llm_spec) == 1, "Expected exactly one tool spec"

    tool_result = inputs[0]
    spec = llm_spec[0]

    # Get function name and id from spec
    function_name = spec["function"]["name"]
    tool_call_id = spec["id"]

    # Construct response content by combining params with tool result
    content = {
        **spec["function"]["parameters"],  # Include all original parameters
        function_name: tool_result,  # Add tool result under function name
    }

    return {
        "role": "tool",
        "tool_call_id": tool_call_id,
        "content": json.dumps(content),  # Content must be string for OpenAI API
    }
