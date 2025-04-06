import json
from typing import Any, Optional, Sequence, TypedDict, Union
from ailets.cons.typeguards import (
    is_content_item_image,
    is_content_item_text,
)
from ailets.cons.atyping import (
    Content,
    ContentItemImage,
    INodeRuntime,
)
from ailets.cons.util import (
    iter_input_objects,
    log,
    read_all,
    read_env_pipe,
    write_all,
)

# https://platform.openai.com/docs/api-reference/images/create

url_tpl = "https://api.openai.com/v1/images/##TASK##"
auth_header = {
    "Authorization": "Bearer {{secret('openai','dalle')}}",
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


async def to_binary_body_in_kv(
    runtime: INodeRuntime, task: str, body: dict[str, Any]
) -> str:
    if task == "variations":
        body = body.copy()
        del body["prompt"]

    key = runtime.get_next_name("query_body")
    fd = await runtime.open_write(key)

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
            await write_all(runtime, fd, b"(before)FIXMEFIXMEFIXME")  # FIXME
            await runtime.pass_through_name_fd(value["key"], fd)
            await write_all(runtime, fd, b"(after)FIXMEFIXMEFIXME")  # FIXME
            await write_all(runtime, fd, b"\r\n")
        else:
            value = str(value)
            await write_all(runtime, fd, f"\r\n\r\n{value}\r\n".encode("utf-8"))

    await write_all(runtime, fd, f"--{boundary}--\r\n".encode("utf-8"))
    await runtime.close(fd)

    return key


class ExtractedPrompt(TypedDict):
    prompt_parts: list[str]
    image: Optional[ContentItemImage]
    mask: Optional[ContentItemImage]


async def read_from_slot(runtime: INodeRuntime, slot_name: str) -> bytes:
    n = runtime.n_of_inputs(slot_name)
    assert n == 1, f"Expected exactly one input for {slot_name}, got {n}"

    fd = await runtime.open_read(slot_name, 0)
    content = await read_all(runtime, fd)
    await runtime.close(fd)
    return content


def update_prompt(prompt: ExtractedPrompt, content: Content) -> None:
    for part in content:
        if is_content_item_text(part):
            prompt["prompt_parts"].append(part["text"])
        elif is_content_item_image(part):
            key = part["key"]
            assert key is not None, "Image has no key"
            assert part["content_type"] == "image/png", "Image must be PNG"
            if prompt["image"] is None:
                prompt["image"] = part
            elif prompt["mask"] is None:
                prompt["mask"] = part
            else:
                raise ValueError(
                    "Too many images. First image is used as image, second as mask."
                )
        else:
            raise ValueError(f"Unsupported content type: {part}")


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
    async for message in iter_input_objects(runtime, ""):
        role = message.get("role")
        if role != "user":
            await log(runtime, "info", f"Skipping message with role {role}")
            continue
        content = message.get("content")
        assert isinstance(content, Sequence), "Content must be a list"
        update_prompt(prompt, content)

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

    output = await runtime.open_write("")
    await write_all(runtime, output, json.dumps(value).encode("utf-8"))
    await runtime.close(output)
