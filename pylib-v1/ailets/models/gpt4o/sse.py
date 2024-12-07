from typing import Any, Mapping

from ailets.cons.atyping import INodeRuntime


class SseHandler:
    def __init__(
        self, init_sse_object: Mapping[str, Any], runtime: INodeRuntime, out_fd: int
    ) -> None:
        self._init_sse_object = init_sse_object
        self._runtime = runtime
        self._out_fd = out_fd

    def handle_sse_object(self, sse_object: Mapping[str, Any]) -> None:
        print("!!!! sse object", sse_object)  # FIXME
        pass


def is_sse_object(obj: Mapping[str, Any]) -> bool:
    return obj.get("object") == "chat.completion.chunk"
