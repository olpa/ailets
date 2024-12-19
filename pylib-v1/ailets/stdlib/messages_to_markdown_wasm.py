from typing import Optional
from ailets.cons.atyping import INodeRuntime
from ailets.cons.util import write_all

import wasmer  # type: ignore[import-untyped]

store: Optional[wasmer.Store] = None
module: Optional[wasmer.Module] = None


def load_wasm_module() -> wasmer.Module:
    global store, module

    import importlib.resources

    wasm_bytes = (
        importlib.resources.files("ailets.wasm")
        .joinpath("messages_to_markdown.wasm")
        .read_bytes()
    )

    if store is None:
        store = wasmer.Store()
    if module is None:
        module = wasmer.Module(store, wasm_bytes)
    return module


async def messages_to_markdown_wasm(runtime: INodeRuntime) -> None:
    fd = await runtime.open_write(None)
    content = "from wasm placeholder"
    await write_all(runtime, fd, content.encode("utf-8"))
    await runtime.close(fd)
