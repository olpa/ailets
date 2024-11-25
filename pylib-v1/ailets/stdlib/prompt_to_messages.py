import json
from ailets.cons.typing import (
    ChatMessage,
    ChatMessageContentImageUrl,
    ChatMessageUser,
    INodeRuntime,
)
from ailets.cons.util import read_all, write_all


def prompt_to_messages(runtime: INodeRuntime) -> None:
    n_prompts = runtime.n_of_streams(None)
    n_types = runtime.n_of_streams("type")

    if n_prompts != n_types:
        raise ValueError("Inputs and type streams have different lengths")

    def to_llm_item(runtime: INodeRuntime, i: int) -> ChatMessage:
        fd = runtime.open_read(None, i)
        content = read_all(runtime, fd).decode("utf-8")
        runtime.close(fd)

        fd = runtime.open_read("type", i)
        content_type = read_all(runtime, fd).decode("utf-8")
        runtime.close(fd)

        if content_type == "text":
            return ChatMessageUser(role="user", content=content)
        elif content_type == "image_url":
            return ChatMessageUser(
                role="user",
                content=[
                    ChatMessageContentImageUrl(
                        type="image_url", image_url={"url": content}
                    )
                ],
            )
        else:
            raise ValueError(f"Unsupported content type: {content_type}")

    messages = [to_llm_item(runtime, i) for i in range(n_prompts)]

    fd = runtime.open_write(None)
    value = json.dumps(messages).encode("utf-8")
    write_all(runtime, fd, value)
    runtime.close(fd)
