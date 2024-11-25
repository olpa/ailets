import json
import base64
import hashlib
from ailets.cons.typeguards import (
    is_chat_message_content_image_url,
    is_chat_message_content_text,
)
from ailets.cons.typing import ChatMessageStructuredContentItem, INodeRuntime
from ailets.cons.util import iter_streams_objects, write_all

need_separator = False


def separator(runtime: INodeRuntime, fd: int) -> None:
    global need_separator
    if need_separator:
        write_all(runtime, fd, b"\n\n")
    else:
        need_separator = True


def rewrite_image_url(runtime: INodeRuntime, url: str) -> str:
    if not url.startswith("data:"):
        return url

    # data:[<mediatype>][;base64],<data>
    try:
        media_type, data = url.split(",", 1)
    except ValueError:
        raise ValueError(f"Invalid image URL, without comma: {url}")

    media_type = media_type.replace("data:", "")
    parts = media_type.split(";", 1)

    if len(parts) == 1:
        media_type = parts[0]
        is_base64 = False
    else:
        media_type = parts[0]
        is_base64 = parts[1] == "base64"

    if is_base64:
        try:
            data_bytes = base64.b64decode(data)
        except Exception as e:
            raise ValueError(f"Invalid base64 data: {e}")
    else:
        data_bytes = data.encode("utf-8")

    # Get file extension based on media type
    extension = {"image/png": ".png", "image/jpeg": ".jpg", "image/gif": ".gif"}.get(
        media_type, ".bin"
    )

    # Generate filename from content hash
    md5_hash = hashlib.md5(data_bytes).hexdigest()
    filename = f"./out/{md5_hash}{extension}"

    # Write to stream
    stream = runtime.open_write(filename)
    write_all(runtime, stream, data_bytes)
    runtime.close(stream)

    return filename


def mixed_content_to_markdown(
    runtime: INodeRuntime,
    fd: int,
    content: ChatMessageStructuredContentItem,
) -> None:
    separator(runtime, fd)

    if isinstance(content, str):
        write_all(runtime, fd, content.encode("utf-8"))
        return

    if is_chat_message_content_text(content):
        write_all(runtime, fd, content["text"].encode("utf-8"))
        return

    if is_chat_message_content_image_url(content):
        url = rewrite_image_url(runtime, content["image_url"]["url"])
        write_all(runtime, fd, f"![image]({url})".encode("utf-8"))
        return

    write_all(runtime, fd, json.dumps(content).encode("utf-8"))


def messages_to_markdown(runtime: INodeRuntime) -> None:
    """Convert chat messages to markdown."""
    global need_separator
    need_separator = False

    fd = runtime.open_write(None)

    try:
        for message in iter_streams_objects(runtime, None):
            content = message["content"]
            if isinstance(content, str):
                separator(runtime, fd)
                write_all(runtime, fd, content.encode("utf-8"))
                continue
            for item in content:
                mixed_content_to_markdown(runtime, fd, item)
    finally:
        runtime.close(fd)
