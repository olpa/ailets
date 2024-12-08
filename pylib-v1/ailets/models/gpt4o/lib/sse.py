from typing import Any, Mapping, Optional, Sequence, cast

from ailets.cons.atyping import ContentItemFunction, INodeRuntime
from ailets.cons.typeguards import is_content_item_function
from ailets.cons.util import write_all


from typing import TypedDict, NotRequired

from ailets.models.gpt4o.lib.tool_calls import ToolCalls


class Delta(TypedDict):
    role: NotRequired[str]
    content: NotRequired[Optional[str]]
    refusal: NotRequired[Optional[str]]


def unwrap_delta(sse_object: Mapping[str, Any]) -> Delta:
    assert isinstance(sse_object, dict), "SSE object must be a dictionary"
    assert "choices" in sse_object, "SSE object must have 'choices' key "
    assert isinstance(sse_object["choices"], list), "'choices' must be a list"
    assert len(sse_object["choices"]) == 1, "'choices' must have exactly one item"
    choice = sse_object["choices"][0]
    assert isinstance(choice, dict), "'choice' must be a dictionary"
    assert choice["index"] == 0, "'index' must be 0"
    assert "delta" in choice, "'choice' must have 'delta' key"
    delta = cast(Delta, choice["delta"])
    return delta


def escape_json_value(s: str) -> str:
    result = []
    for c in s:
        if c == "\\":
            result.append("\\\\")
        elif c == '"':
            result.append('\\"')
        elif c == "\n":
            result.append("\\n")
        elif ord(c) < 0x20:
            result.append(f"\\u{ord(c):04x}")
        else:
            result.append(c)
    return "".join(result)



class SseHandler:
    def __init__(self, runtime: INodeRuntime, out_fd: int) -> None:
        self.runtime = runtime
        self.out_fd = out_fd
        self.message_is_started = False
        self.tool_calls_started = False
        self.tool_calls = ToolCalls()

    async def handle_sse_object(self, sse_object: Mapping[str, Any]) -> None:
        delta = unwrap_delta(sse_object)
        if not self.message_is_started:
            role = delta["role"]
            assert role is not None, "SSE message must start with a role"

            header = f'{{"role":"{role}","content":[{{"type":"text","text":"'.encode(
                "utf-8"
            )
            await write_all(self.runtime, self.out_fd, header)
            self.message_is_started = True

        content = delta.get("content")
        if content:
            escaped = escape_json_value(content)
            await write_all(self.runtime, self.out_fd, escaped.encode("utf-8"))

        tool_calls = delta.get("tool_calls")
        if tool_calls:
            assert isinstance(tool_calls, list), "Tool calls must be a list"
            if not self.tool_calls_started:
                self.tool_calls_started = True
                self.tool_calls.extend(tool_calls)
            else:
                self.tool_calls.delta(tool_calls)

    async def done(self) -> None:
        assert self.message_is_started, "Message is not started"
        await write_all(self.runtime, self.out_fd, b'"}]}')


def is_sse_object(obj: Mapping[str, Any]) -> bool:
    return obj.get("object") == "chat.completion.chunk"
