import json
import base64
import hashlib
from io import BytesIO
from urllib.parse import urlparse
from ailets.cons.typeguards import (
    is_chat_message_content_image_url,
    is_chat_message_content_text,
)
from ailets.cons.typing import ChatMessageStructuredContentItem, INodeRuntime
from ailets.cons.util import iter_streams_objects

need_separator = False


def separator(output: BytesIO) -> None:
    global need_separator
    if need_separator:
        output.write(b"\n\n")
    else:
        need_separator = True


def rewrite_image_url(runtime: INodeRuntime, url: str) -> str:
    if not url.startswith("data:"):
        return url

    parsed = urlparse(url)

    try:
        media_type, data = parsed.path.split(",", 1)
    except ValueError:
        media_type = parsed.path
        data = ""

    parts = media_type.split(";", 1)
    media_type = parts[0]  # Extract the core media type

    if media_type.endswith(";base64"):
        media_type = media_type[:-7]  # Remove ;base64 suffix
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
    stream.write(data_bytes)
    runtime.close_write(filename)

    return filename


def mixed_content_to_markdown(
    runtime: INodeRuntime,
    output: BytesIO,
    content: ChatMessageStructuredContentItem,
) -> None:
    separator(output)

    if isinstance(content, str):
        output.write(content)
        return

    if is_chat_message_content_text(content):
        output.write(content["text"].encode("utf-8"))
        return

    if is_chat_message_content_image_url(content):
        url = rewrite_image_url(runtime, content["image_url"]["url"])
        output.write(f"![image]({url})".encode("utf-8"))
        return

    output.write(json.dumps(content).encode("utf-8"))


def messages_to_markdown(runtime: INodeRuntime) -> None:
    """Convert chat messages to markdown."""
    global need_separator
    need_separator = False

    output = runtime.open_write(None)

    for message in iter_streams_objects(runtime, None):
        content = message["content"]
        if isinstance(content, str):
            separator(output)
            output.write(content.encode("utf-8"))
            continue
        for item in content:
            mixed_content_to_markdown(runtime, output, item)

    runtime.close_write(None)
