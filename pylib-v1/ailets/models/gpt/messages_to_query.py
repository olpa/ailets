import base64
import json
from typing import Any, List, Sequence, Tuple
from ailets.atyping import (
    Content,
    ContentItem,
    ContentItemFunction,
    INodeRuntime,
    StdHandles,
)
from ailets.io.input_reader import iter_input_objects, read_all
from ailets.cons.util import write_all
from ailets.models.gpt.lib.typing import GptContentItem, GptMessage

url = "https://api.openai.com/v1/chat/completions"
method = "POST"
headers = {
    "Content-type": "application/json",
    "Authorization": "Bearer {{secret}}",
}


async def rewrite_content_item(
    runtime: INodeRuntime,
    item: ContentItem,
) -> GptContentItem:
    if item["type"] == "text":
        return item
    assert item["type"] == "image", "Only text and image are supported"

    if url := item.get("url"):
        return {
            "type": "image_url",
            "image_url": {
                "url": url,
            },
        }

    key = item.get("key")
    assert key, "Image URL or key is required"

    fd = await runtime.open_read(key)
    data = await read_all(runtime, fd)
    await runtime.close(fd)

    b64_data = base64.b64encode(data).decode("utf-8")
    data_url = f"data:{item['content_type']};base64,{b64_data}"
    return {
        "type": "image_url",
        "image_url": {"url": data_url},
    }


async def rewrite_content(
    runtime: INodeRuntime,
    content: Content,
) -> Tuple[Sequence[GptContentItem], Sequence[ContentItemFunction]]:
    new_content: List[GptContentItem] = []
    tool_calls: List[ContentItemFunction] = []
    for item in content:
        if item["type"] == "function":
            tool_calls.append(item)
        else:
            new_content.append(await rewrite_content_item(runtime, item))
    return new_content, tool_calls


async def get_overrides(runtime: INodeRuntime) -> dict[str, Any]:
    known_model_params = [
        "messages",
        "model",
        "store",
        "metadata",
        "frequency_penalty",
        "logit_bias",
        "logprobs",
        "top_logprobs",
        "max_tokens",
        "max_completion_tokens",
        "n",
        "modalities",
        "prediction",
        "audio",
        "presence_penalty",
        "response_format",
        "seed",
        "service_tier",
        "stop",
        "stream",
        "stream_options",
        "temperature",
        "top_p",
        "tools",
        "tool_choice",
        "parallel_tool_calls",
        "user",
        "function_call",
        "functions",
    ]
    overrides: dict[str, Any] = {}
    async for cfg in iter_input_objects(runtime, StdHandles.env):
        gpt_cfg = cfg.get("gpt")
        if not gpt_cfg:
            continue
        for param in known_model_params:
            if param in gpt_cfg:
                overrides[param] = gpt_cfg[param]
    return overrides


async def messages_to_query(runtime: INodeRuntime) -> None:
    """Convert chat messages into a query."""
    raise ValueError("Python-actor for GPT should not be used anymore")

    messages: list[GptMessage] = []
    async for message in iter_input_objects(runtime, StdHandles.stdin):
        new_message: GptMessage = message.copy()  # type: ignore[assignment]
        if "content" in message:
            new_content, tool_calls = await rewrite_content(runtime, message["content"])
            new_message["content"] = new_content
            if tool_calls:
                new_message["tool_calls"] = tool_calls

        messages.append(new_message)

    tools = []
    async for toolspec in iter_input_objects(runtime, "toolspecs"):
        tools.append(
            {
                "type": "function",
                "function": toolspec,
            }
        )
    tools_param = {"tools": tools} if tools else {}

    body = {
        "model": "gpt-4o-mini",
        "messages": messages,
        "stream": True,
        **tools_param,
    }
    body.update(await get_overrides(runtime))

    value = {
        "url": url,
        "method": method,
        "headers": headers,
        "body": body,
    }

    await write_all(runtime, StdHandles.stdout, json.dumps(value).encode("utf-8"))
