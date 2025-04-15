import asyncio
import json
from typing import Any, Dict
from ailets.cons.atyping import INodeRuntime, StdHandles
from ailets.cons.util import write_all
from ailets.cons.input_reader import iter_input_objects


async def prompt_to_messages(runtime: INodeRuntime) -> None:
    role_to_content: Dict[str, Any] = {}
    async for content_item in iter_input_objects(runtime, StdHandles.stdin):
        role = content_item.get("role", "user")
        role_to_content.setdefault(role, []).append(content_item)

    keys = list(role_to_content.keys())
    keys.sort()
    messages = list(
        map(
            lambda key: {
                "role": key,
                "content": role_to_content[key],
            },
            keys,
        )
    )
    # FIXME
    if "FIXME" in json.dumps(list(messages)):
        for i in range(3):
            await write_all(
                runtime,
                StdHandles.stdout,
                json.dumps(messages).encode("utf-8"),
            )
            await asyncio.sleep(0.1)
        for i in range(3):
            await write_all(
                runtime,
                StdHandles.stdout,
                "FIXME_SHOULD_FAIL\n".encode("utf-8"),
            )
            await asyncio.sleep(0.1)
        await write_all(
            runtime,
            StdHandles.stdout,
            "just some text to see writing to a broken pipe\n".encode("utf-8"),
        )

    await write_all(
        runtime,
        StdHandles.stdout,
        json.dumps(messages).encode("utf-8"),
    )
