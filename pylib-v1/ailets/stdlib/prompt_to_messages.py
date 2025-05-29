import json
from ailets.atyping import INodeRuntime, StdHandles
from ailets.cons.util import write_all
from ailets.io.input_reader import iter_input_objects


async def prompt_to_messages(runtime: INodeRuntime) -> None:
    last_role = None
    should_close_messages = False

    async def maybe_close_messages() -> None:
        if should_close_messages:
            await write_all(runtime, StdHandles.stdout, b"]}\n")

    async for content_item in iter_input_objects(runtime, StdHandles.stdin):
        role = last_role
        is_ctl_node = (
            isinstance(content_item, list)
            and len(content_item) > 0
            and content_item[0].get("type") == "ctl"
        )
        if is_ctl_node:
            role = content_item[1]["role"]  # type: ignore[index]
        if role is None:
            role = "user"
        if role != last_role:
            await maybe_close_messages()
            should_close_messages = True
            last_role = role
            await write_all(
                runtime,
                StdHandles.stdout,
                f'{{"role": "{last_role}", "content": [\n'.encode("utf-8"),
            )
        if is_ctl_node:
            continue

        await write_all(
            runtime, StdHandles.stdout, json.dumps(content_item).encode("utf-8")
        )
        await write_all(runtime, StdHandles.stdout, b",\n")

    await maybe_close_messages()
