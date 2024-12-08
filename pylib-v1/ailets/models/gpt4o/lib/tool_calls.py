from typing import Any, Mapping, Optional, Sequence
import json
from ailets.cons.atyping import ContentItemFunction, INodeRuntime, ChatMessage
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

    def to_dag(self, runtime: INodeRuntime) -> None:
        """Process tool calls and update the DAG."""
        if not self.tool_calls:
            return

        dagops = runtime.dagops()
        
        #
        # Put "tool_calls" to the "chat history"
        #
        tool_calls_message: ChatMessage = {
            "role": "assistant",
            "content": self.tool_calls,
        }
        tool_calls_node = dagops.add_value_node(
            json.dumps([tool_calls_message]).encode("utf-8"),
            explain='Feed "tool_calls" from output to input',
        )
        dagops.alias(".chat_messages", tool_calls_node)

        #
        # Instantiate tools and connect them to the "chat history"
        #
        for tool_call in self.tool_calls:
            tool_spec_node_name = dagops.add_value_node(
                json.dumps(tool_call).encode("utf-8"),
                explain="Tool call spec from llm",
            )

            tool_name = tool_call["function"]["name"]
            tool_final_node_name = dagops.instantiate_with_deps(
                f".tool.{tool_name}", {".tool_input": tool_spec_node_name}
            )

            tool_msg_node_name = dagops.instantiate_with_deps(
                ".toolcall_to_messages",
                {
                    ".llm_tool_spec": tool_spec_node_name,
                    ".tool_output": tool_final_node_name,
                },
            )
            dagops.alias(".chat_messages", tool_msg_node_name)
        
        #
        # Re-run the model
        #
        rerun_node_name = dagops.instantiate_with_deps(".gpt4o", {})
        dagops.alias(".model_output", rerun_node_name)
