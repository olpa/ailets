import json
from typing import Optional, Sequence, TypedDict
from ailets.cons.typeguards import (
    is_content_item_image,
    is_content_item_text,
)
from ailets.cons.typing import (
    Content,
    INodeRuntime,
)
from ailets.cons.util import (
    iter_streams_objects,
    log,
    read_all,
    read_env_stream,
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


def task_to_body(runtime: INodeRuntime, task: str, body: dict) -> dict | str:
    if task == "generations":
        return body

    if task == "variations":
        body = body.copy()
        del body["prompt"]

    stream_name = runtime.get_next_name("query_body")
    fd = runtime.open_write(stream_name)

    for key, value in body.items():
        write_all(runtime, fd, f"--{boundary}\n".encode("utf-8"))
        write_all(
            runtime,
            fd,
            f'Content-Disposition: form-data; name="{key}"\n\n'.encode("utf-8"),
        )
        if isinstance(value, bytes):
            write_all(runtime, fd, value)
        else:
            value = str(value)
            write_all(runtime, fd, f"{value}\n".encode("utf-8"))

    write_all(runtime, fd, f"--{boundary}--\n".encode("utf-8"))
    runtime.close(fd)

    return stream_name


class ExtractedPrompt(TypedDict):
    prompt_parts: list[str]
    image: Optional[bytes]
    mask: Optional[bytes]


def read_stream(runtime: INodeRuntime, stream_name: str) -> bytes:
    n = runtime.n_of_streams(stream_name)
    assert n == 1, f"Expected exactly one stream for {stream_name}, got {n}"

    fd = runtime.open_read(stream_name, 0)
    content = read_all(runtime, fd)
    runtime.close(fd)
    return content


def update_prompt(
    runtime: INodeRuntime,
    prompt: ExtractedPrompt,
    content: Content,
) -> None:
    for part in content:
        if is_content_item_text(part):
            prompt["prompt_parts"].append(part["text"])
        elif is_content_item_image(part):
            stream = part["stream"]
            assert stream is not None, "Image has no stream"
            if prompt["image"] is None:
                prompt["image"] = read_stream(runtime, stream)
            elif prompt["mask"] is None:
                prompt["mask"] = read_stream(runtime, stream)
            else:
                raise ValueError(
                    "Too many images. First image is used as image, second as mask."
                )
        else:
            raise ValueError(f"Unsupported content type: {part}")


def messages_to_query(runtime: INodeRuntime) -> None:
    """Convert prompt message into a DALL-E query."""
    params = read_env_stream(runtime)
    task = params.get("dalle_task", "generations")
    assert task in (
        "generations",
        "variations",
        "edits",
    ), "Invalid DALL-E task, expected one of: generations, variations, edits"

    prompt = ExtractedPrompt(prompt_parts=[], image=None, mask=None)
    for message in iter_streams_objects(runtime, None):
        role = message.get("role")
        if role != "user":
            log(runtime, "info", f"Skipping message with role {role}")
            continue
        content = message.get("content")
        assert isinstance(content, Sequence), "Content must be a list"
        update_prompt(runtime, prompt, content)

    if not len(prompt["prompt_parts"]) and task != "variations":
        raise ValueError("No user prompt found in messages")

    body = task_to_body(
        runtime,
        task,
        {
            "model": params.get("model", "dall-e-3"),
            "prompt": " ".join(prompt["prompt_parts"]),
            "n": params.get("n", 1),
            "response_format": params.get("response_format", "url"),
            **({"image": prompt["image"]} if prompt["image"] is not None else {}),
            **({"mask": prompt["mask"]} if prompt["mask"] is not None else {}),
        },
    )
    body_field = "body_stream" if isinstance(body, str) else "body"

    value = {
        "url": task_to_url(task),
        "method": "POST",
        "headers": task_to_headers(task),
        body_field: body,
    }

    output = runtime.open_write(None)
    write_all(runtime, output, json.dumps(value).encode("utf-8"))
    runtime.close(output)
