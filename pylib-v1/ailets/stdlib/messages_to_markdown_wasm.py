import asyncio
import importlib.resources
from typing import Optional
from ailets.cons.atyping import INodeRuntime
from ailets.cons.node_runtime_wasm import BufToStr, fill_wasm_import_object

import wasmer  # type: ignore[import-untyped]

wasm_bytes: Optional[bytes] = None


def load_wasm_module() -> None:
    global wasm_bytes

    if wasm_bytes is None:
        wasm_bytes = (
            importlib.resources.files("ailets.wasm")
            .joinpath("messages_to_markdown.wasm")
            .read_bytes()
        )


async def messages_to_markdown_wasm(runtime: INodeRuntime) -> None:
    assert wasm_bytes is not None, "WASM module not loaded"

    def init_and_run() -> None:
        # Set up WASM environment
        store = wasmer.Store()
        module = wasmer.Module(store, wasm_bytes)
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

        err_ptr = run_fn()
        if err_ptr:
            err = buf_to_str.get_string(err_ptr)
            raise RuntimeError(f"Actor error: {err}")

    # Run the WASM function
    await asyncio.to_thread(init_and_run)
