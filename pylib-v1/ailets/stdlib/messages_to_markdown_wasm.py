from typing import Optional
from ailets.cons.atyping import INodeRuntime
from ailets.cons.node_runtime_wasm import BufToStr, fill_wasm_import_object

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
    assert module is not None, "WASM module not loaded"

    # Set up WASM environment
    import_object = wasmer.ImportObject()
    buf_to_str = BufToStr()
    fill_wasm_import_object(store, import_object, buf_to_str, runtime)

    # Create WASM instance
    instance = wasmer.Instance(module, import_object)
    run_fn = instance.exports.messages_to_markdown

    # Set up memory for string handling
    memory = instance.exports.memory
    if not isinstance(memory, wasmer.Memory):
        raise ValueError("Memory is not a Memory")
    buf_to_str.set_memory(memory)

    # Run the WASM function
    run_fn()
