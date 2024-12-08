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
    tool_calls: NotRequired[Optional[list[dict[str, Any]]]]


def is_sse_object(obj: Mapping[str, Any]) -> bool:
    return obj.get("object") == "chat.completion.chunk"


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
    def __init__(self, runtime: INodeRuntime, tool_calls: ToolCalls, out_fd: int) -> None:
        self.runtime = runtime
        self.tool_calls = tool_calls
        self.out_fd = out_fd

        self.role: Optional[str] = None
        self.message_is_started = False
        self.tool_calls_started = False

    async def handle_sse_object(self, sse_object: Mapping[str, Any]) -> None:
        delta = unwrap_delta(sse_object)

        role = delta.get("role")
        if role:
            assert not self.message_is_started, "SSE with role, but the message is already started"
            assert self.role is None, "SSE with role, but the role is already set"
            self.role = role

        content = delta.get("content")
        if content:
            if not self.message_is_started:
                assert self.role is not None, "SSE with content, but the 'role' is not set"
                self.message_is_started = True
                header = f'{{"role":"{self.role}","content":[{{"type":"text","text":"'.encode(
                    "utf-8"
                )
                await write_all(self.runtime, self.out_fd, header)

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
        if self.message_is_started:
            await write_all(self.runtime, self.out_fd, b'"}]}')

