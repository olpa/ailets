import base64
import json
from typing import Optional, Sequence, TypedDict
from ailets.cons.typeguards import (
    is_chat_message_content_image_url,
    is_chat_message_content_text,
)
from ailets.cons.typing import (
    ChatMessage,
    ChatMessageContent,
    ChatMessageContentPlainText,
    INodeRuntime,
)
from ailets.cons.util import log, read_env_stream, read_all, write_all

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


def task_to_body(task: str, body: dict) -> dict | str:
    if task == "generations":
        return body
    if task == "variations":
        body = body.copy()
        del body["prompt"]

    form_data = []
    for key, value in body.items():
        form_data.append(
            (
                f"--{boundary}\n"
                f'Content-Disposition: form-data; name="{key}"\n\n'
                f"{value}\n"
            )
        )
    form_data.append(f"--{boundary}--\n")

    return "".join(form_data)


class ExtractedPrompt(TypedDict):
    prompt_parts: list[str]
    image: Optional[bytes]
    mask: Optional[bytes]


def decode_data_url(url: str) -> bytes:
    if not url.startswith("data:"):
        raise ValueError("URL must be a data URL starting with 'data:'")
    try:
        _, data = url.split(",", 1)
    except ValueError:
        raise ValueError("Invalid data URL format - missing comma separator")
    try:
        return base64.b64decode(data)
    except Exception as e:
        raise ValueError(f"Invalid base64 data: {str(e)}")


def update_prompt(
    runtime: INodeRuntime,
    prompt: ExtractedPrompt,
    content: Optional[ChatMessageContent],
) -> None:
    if isinstance(content, ChatMessageContentPlainText):
        prompt["prompt_parts"].append(content)
        return
    if isinstance(content, Sequence):
        for part in content:
            if isinstance(part, ChatMessageContentPlainText):
                prompt["prompt_parts"].append(part)
                continue
            if is_chat_message_content_text(part):
                prompt["prompt_parts"].append(part["text"])
            elif is_chat_message_content_image_url(part):
                url = part["image_url"]["url"]
                if prompt["image"] is None:
                    prompt["image"] = decode_data_url(url)
                elif prompt["mask"] is None:
                    prompt["mask"] = decode_data_url(url)
                else:
                    raise ValueError(
                        "Too many images. First image is used as image, second as mask."
                    )
            else:
                raise ValueError(f"Unsupported content type: {part}")
        return
    raise ValueError(f"Unsupported content type: {type(content)}")


def messages_to_query(runtime: INodeRuntime) -> None:
    """Convert prompt message into a DALL-E query."""

    prompt = ExtractedPrompt(prompt_parts=[], image=None, mask=None)

    for i in range(runtime.n_of_streams(None)):
        stream = runtime.open_read(None, i)
        messages: Sequence[ChatMessage] = json.loads(read_all(runtime, stream).decode("utf-8"))
        runtime.close(stream)
        for message in messages:
            role = message.get("role")
            if role != "user":
                log(runtime, "info", f"Skipping message with role {role}")
                continue
            update_prompt(runtime, prompt, message.get("content"))

    if not len(prompt["prompt_parts"]):
        raise ValueError("No user prompt found in messages")

    params = read_env_stream(runtime)

    task = params.get("dalle_task", "generations")
    assert task in (
        "generations",
        "variations",
        "edits",
    ), "Invalid DALL-E task, expected one of: generations, variations, edits"

    value = {
        "url": task_to_url(task),
        "method": "POST",
        "headers": task_to_headers(task),
        "body": task_to_body(
            task,
            {
                "model": params.get("model", "dall-e-3"),
                "prompt": " ".join(prompt["prompt_parts"]),
                "n": params.get("n", 1),
                "response_format": params.get("response_format", "url"),
                **({"image": prompt["image"]} if prompt["image"] is not None else {}),
                **({"mask": prompt["mask"]} if prompt["mask"] is not None else {}),
            },
        ),
    }

    output = runtime.open_write(None)
    write_all(runtime, output, json.dumps(value).encode("utf-8"))
    runtime.close(output)
