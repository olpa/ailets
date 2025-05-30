import json
from typing import Any, Optional, TypedDict, Union
from ailets.io.input_reader import iter_input_objects
from ailets.cons.typeguards import (
    is_content_item_image,
    is_content_item_text,
)
from ailets.atyping import (
    ContentItem,
    ContentItemImage,
    INodeRuntime,
    StdHandles,
)
from ailets.cons.util import (
    write_all,
)
from ailets.io.input_reader import read_all, read_env_pipe


# https://platform.openai.com/docs/api-reference/images/create

url_tpl = "https://api.openai.com/v1/images/##TASK##"
auth_header = {
    "Authorization": "Bearer {{secret}}",
}

boundary = "----AiletsBoundary7MA4YWxkTrZu0gW"


def task_to_url(task: str) -> str:
    return url_tpl.replace("##TASK##", task)


def task_to_headers(task: str) -> dict[str, str]:
    if task == "generations":
        return {**auth_header, "Content-type": "application/json"}
    else:
        return {
            **auth_header,
            "Content-type": f"multipart/form-data; boundary={boundary}",
        }


async def copy_image_to_fd(runtime: INodeRuntime, key: str, fd: int) -> None:
    fd_in = await runtime.open_read(key)
    buffer = await read_all(runtime, fd_in)
    await runtime.close(fd_in)
    await write_all(runtime, fd, buffer)


async def to_binary_body_in_kv(
    runtime: INodeRuntime, task: str, body: dict[str, Any]
) -> str:
    if task == "variations":
        body = body.copy()
        del body["prompt"]

    body_key = runtime.get_next_name("spool/query_body")
    fd = await runtime.open_write(body_key)

    for key, value in body.items():
        await write_all(runtime, fd, f"--{boundary}\r\n".encode("utf-8"))
        await write_all(
            runtime,
            fd,
            f'Content-Disposition: form-data; name="{key}"'.encode("utf-8"),
        )
        if is_content_item_image(value):
            await write_all(runtime, fd, b'; filename="image.png"\r\n')
            await write_all(runtime, fd, b"Content-Type: image/png\r\n\r\n")
            await copy_image_to_fd(runtime, value[1]["image_key"], fd)
            await write_all(runtime, fd, b"\r\n")
        else:
            value = str(value)
            await write_all(runtime, fd, f"\r\n\r\n{value}\r\n".encode("utf-8"))

    await write_all(runtime, fd, f"--{boundary}--\r\n".encode("utf-8"))
    await runtime.close(fd)

    return body_key


class ExtractedPrompt(TypedDict):
    prompt_parts: list[str]
    image: Optional[ContentItemImage]
    mask: Optional[ContentItemImage]


def update_prompt(prompt: ExtractedPrompt, content_item: ContentItem) -> None:
    if is_content_item_text(content_item):
        prompt["prompt_parts"].append(content_item[1]["text"])
        return
    if is_content_item_image(content_item):
        key = content_item[1]["image_key"]
        assert key is not None, "Image has no key"
        assert content_item[0]["content_type"] == "image/png", "Image must be PNG"
        if prompt["image"] is None:
            prompt["image"] = content_item
        elif prompt["mask"] is None:
            prompt["mask"] = content_item
        else:
            raise ValueError(
                "Too many images. First image is used as image, second as mask."
            )


async def messages_to_query(runtime: INodeRuntime) -> None:
    """Convert prompt message into a DALL-E query."""
    params = await read_env_pipe(runtime)
    task = params.get("dalle_task", "generations")
    assert task in (
        "generations",
        "variations",
        "edits",
    ), "Invalid DALL-E task, expected one of: generations, variations, edits"

    prompt = ExtractedPrompt(prompt_parts=[], image=None, mask=None)
    async for obj in iter_input_objects(runtime, StdHandles.stdin):
        content_item: ContentItem = obj  # type: ignore
        update_prompt(prompt, content_item)

    if not len(prompt["prompt_parts"]) and task != "variations":
        raise ValueError("No user prompt found in messages")

    shared_params = {
        "prompt": " ".join(prompt["prompt_parts"]),
        "n": params.get("n", 1),
        "response_format": params.get("response_format", "url"),
    }

    body: Union[dict[str, Any], str]
    if task == "generations":
        body_field = "body"
        body = shared_params
    else:
        body_field = "body_key"
        body = await to_binary_body_in_kv(
            runtime,
            task,
            {
                **shared_params,
                **({"image": prompt["image"]} if prompt["image"] is not None else {}),
                **({"mask": prompt["mask"]} if prompt["mask"] is not None else {}),
            },
        )

    value = {
        "url": task_to_url(task),
        "method": "POST",
        "headers": task_to_headers(task),
        body_field: body,
    }

    await write_all(runtime, StdHandles.stdout, json.dumps(value).encode("utf-8"))
