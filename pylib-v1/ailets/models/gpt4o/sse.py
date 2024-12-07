from typing import Any, Mapping, Optional, cast

from ailets.cons.atyping import INodeRuntime
from ailets.cons.util import write_all


from typing import TypedDict, NotRequired


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
    def __init__(
        self, init_sse_object: Mapping[str, Any], runtime: INodeRuntime, out_fd: int
    ) -> None:
        self.init_sse_object = init_sse_object
        self.runtime = runtime
        self.out_fd = out_fd
        self.message_is_started = False

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

    async def done(self) -> None:
        assert self.message_is_started, "Message is not started"
        await write_all(self.runtime, self.out_fd, b'"}]}')


def is_sse_object(obj: Mapping[str, Any]) -> bool:
    return obj.get("object") == "chat.completion.chunk"