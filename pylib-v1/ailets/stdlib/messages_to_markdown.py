import json
import base64
import hashlib
from ailets.cons.typeguards import (
    is_content_item_ctl,
    is_content_item_image,
    is_content_item_text,
)
from ailets.atyping import (
    ContentItem,
    ContentItemImage,
    INodeRuntime,
    StdHandles,
)
from ailets.cons.util import write_all
from ailets.io.input_reader import iter_input_objects

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
    if key := image[1].get("image_key"):
        raise ValueError("Not implemented: Output image reference by key")
        return key

    url = image[1].get("image_url")
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
    content_item: ContentItem,
) -> None:
    await separator(runtime, fd)

    if is_content_item_ctl(content_item):
        return

    if is_content_item_text(content_item):
        await write_all(runtime, fd, content_item[1]["text"].encode("utf-8"))
        await write_all(runtime, fd, b"\n")
        await write_all(runtime, fd, b"\n")
        return

    if is_content_item_image(content_item):
        url = await rewrite_image_url(runtime, content_item)
        await write_all(runtime, fd, f"![image]({url})\n".encode("utf-8"))
        return

    await write_all(runtime, fd, json.dumps(content_item).encode("utf-8"))


async def messages_to_markdown(runtime: INodeRuntime) -> None:
    """Convert chat messages to markdown."""
    global need_separator
    need_separator = False

    async for obj in iter_input_objects(runtime, StdHandles.stdin):
        content_item: ContentItem = obj  # type: ignore
        await content_to_markdown(runtime, StdHandles.stdout, content_item)
