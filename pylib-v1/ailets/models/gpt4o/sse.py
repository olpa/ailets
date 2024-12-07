from typing import Any, Mapping, cast

from ailets.cons.atyping import INodeRuntime
from ailets.cons.util import write_all


from typing import TypedDict, Optional


class Delta(TypedDict):
    role: Optional[str]
    content: Optional[str]
    refusal: Optional[str]


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


class SseHandler:
    def __init__(
        self, init_sse_object: Mapping[str, Any], runtime: INodeRuntime, out_fd: int
    ) -> None:
        self.init_sse_object = init_sse_object
        self.runtime = runtime
        self.out_fd = out_fd

    async def handle_sse_object(self, sse_object: Mapping[str, Any]) -> None:
        delta = unwrap_delta(sse_object)
        print("!!!! delta", delta)  # FIXME
        pass

    async def done(self) -> None:
        await write_all(self.runtime, self.out_fd, b"}")


def is_sse_object(obj: Mapping[str, Any]) -> bool:
    return obj.get("object") == "chat.completion.chunk"
