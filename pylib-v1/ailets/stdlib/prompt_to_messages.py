import json
from ailets.atyping import INodeRuntime, StdHandles
from ailets.cons.util import write_all
from ailets.io.input_reader import iter_input_objects


async def prompt_to_messages(runtime: INodeRuntime) -> None:
    last_role = None

    async for content_item in iter_input_objects(runtime, StdHandles.stdin):
        should_start_message = False
        role = last_role

        is_ctl_node = (
            isinstance(content_item, list)
            and len(content_item) > 0
            and content_item[0].get("type") == "ctl"
        )
        if is_ctl_node:
            role = content_item[1]["role"]  # type: ignore[index]
            should_start_message = True
        if role != last_role:
            should_start_message = True
        if role is None:
            role = "user"
            should_start_message = True

        if should_start_message:
            last_role = role
            await write_all(
                runtime,
                StdHandles.stdout,
                f'{{"type": "ctl", "role": "{last_role}"}}\n'.encode("utf-8"),
            )
        if is_ctl_node:
            continue

        if should_start_message:
            await write_all(runtime, StdHandles.stdout, b"\n")
        else:
            await write_all(runtime, StdHandles.stdout, b",\n")

        await write_all(
            runtime, StdHandles.stdout, json.dumps(content_item).encode("utf-8")
        )
