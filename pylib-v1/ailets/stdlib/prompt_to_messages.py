import json
from typing import Any, Dict
from ailets.cons.atyping import INodeRuntime
from ailets.cons.util import write_all
from ailets.cons.input_reader import iter_input_objects


async def prompt_to_messages(runtime: INodeRuntime) -> None:
    role_to_content: Dict[str, Any] = {}
    async for content_item in iter_input_objects(runtime, ""):
        role = content_item.get("role", "user")
        role_to_content.setdefault(role, []).append(content_item)

    keys = list(role_to_content.keys())
    keys.sort()
    messages = map(
        lambda key: {
            "role": key,
            "content": role_to_content[key],
        },
        keys,
    )

    fd_out = await runtime.open_write("")
    await write_all(runtime, fd_out, json.dumps(list(messages)).encode("utf-8"))
    await runtime.close(fd_out)
