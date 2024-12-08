from typing import Any, Mapping, Optional, Sequence

from ailets.cons.atyping import ContentItemFunction
from ailets.cons.typeguards import is_content_item_function


class ToolCalls:
    def __init__(self) -> None:
        self.tool_calls: list[ContentItemFunction] = []

    def extend(self, tool_calls: Sequence[Mapping[str, Any]]) -> None:
        for tool_call in tool_calls:
            assert tool_call["index"] == len(
                self.tool_calls
            ), "Tool call indices must be sequential"
            assert is_content_item_function(tool_call), "Tool call must be a function"
            self.tool_calls.append(tool_call)

    def delta(self, tool_calls: Optional[Sequence[Mapping[str, Any]]]) -> None:
        if tool_calls is None:
            return
        for tool_call in tool_calls:
            index = tool_call["index"]
            if index < 0 or index >= len(self.tool_calls):
                raise ValueError(f"Tool call index {index} is out of range")
            base_tool_call = self.tool_calls[index]
            assert "function" in tool_call, "Tool call must have 'function' key"
            function = tool_call["function"]
            assert isinstance(function, dict), "'function' must be a dictionary"
            assert list(function.keys()) == [
                "arguments"
            ], "'function' must only have 'arguments' key"

            base_tool_call["function"]["arguments"] = function["arguments"]

    def get_tool_calls(self) -> list[ContentItemFunction]:
        return self.tool_calls