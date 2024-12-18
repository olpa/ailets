from ailets.cons.atyping import INodeRuntime
from ailets.cons.util import write_all


async def messages_to_markdown_wasm(runtime: INodeRuntime) -> None:
    fd = await runtime.open_write(None)
    content = "from wasm placeholder"
    await write_all(runtime, fd, content.encode("utf-8"))
    await runtime.close(fd)
