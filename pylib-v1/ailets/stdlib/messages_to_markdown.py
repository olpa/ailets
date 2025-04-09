import json
import base64
import hashlib
from ailets.cons.typeguards import is_content_item_image, is_content_item_text
from ailets.cons.atyping import (
    Content,
    ContentItemImage,
    INodeRuntime,
)
from ailets.cons.util import write_all
from ailets.cons.input_reader import iter_input_objects

need_separator = False


async def separator(runtime: INodeRuntime, fd: int) -> None:
    global need_separator
    if need_separator:
        await write_all(runtime, fd, b"\n\n")
    else:
        need_separator = True


def get_extension(media_type: str) -> str:
    extension_map = {"image/png": ".png", "image/jpeg": ".jpg"}
    return extension_map.get(media_type, ".bin")


async def rewrite_image_url(runtime: INodeRuntime, image: ContentItemImage) -> str:
    if key := image.get("key"):
        raise ValueError("Not implemented: Output image reference by key")
        return key

    url = image.get("url")
    if not url:
        raise ValueError("Image has no URL or kv key")

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

    # Generate filename from content hash
    md5_hash = hashlib.md5(data_bytes).hexdigest()
    filename = f"out/{md5_hash}{get_extension(media_type)}"

    # Write to kv
    fd_out = await runtime.open_write(filename)
    await write_all(runtime, fd_out, data_bytes)
    await runtime.close(fd_out)

    return filename


async def content_to_markdown(
    runtime: INodeRuntime,
    fd: int,
    content: Content,
) -> None:
    await separator(runtime, fd)

    if is_content_item_text(content):
        await write_all(runtime, fd, content["text"].encode("utf-8"))
        return

    if is_content_item_image(content):
        url = await rewrite_image_url(runtime, content)
        await write_all(runtime, fd, f"![image]({url})".encode("utf-8"))
        return

    await write_all(runtime, fd, json.dumps(content).encode("utf-8"))


async def messages_to_markdown(runtime: INodeRuntime) -> None:
    """Convert chat messages to markdown."""
    global need_separator
    need_separator = False

    fd = await runtime.open_write("")

    try:
        async for message in iter_input_objects(runtime, ""):
            content = message["content"]
            if isinstance(content, str):
                await separator(runtime, fd)
                await write_all(runtime, fd, content.encode("utf-8"))
                continue
            for item in content:
                await content_to_markdown(runtime, fd, item)
    finally:
        await runtime.close(fd)
