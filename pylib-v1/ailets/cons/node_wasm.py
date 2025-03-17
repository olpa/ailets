import asyncio
import importlib.resources
from typing import Callable, Awaitable, Dict
import wasmer  # type: ignore[import-untyped]
from ailets.cons.atyping import INodeRuntime, IWasmRegistry
from ailets.cons.node_runtime_wasm import BufToStr, fill_wasm_import_object


class WasmRegistry:
    def __init__(self) -> None:
        self._modules: Dict[str, bytes] = {}

    def get_module(self, name: str) -> bytes:
        """Get a WASM module by name."""
        if name in self._modules:
            return self._modules[name]
        wasm_bytes = (
            importlib.resources.files("ailets.wasm").joinpath(name).read_bytes()
        )
        self._modules[name] = wasm_bytes
        return wasm_bytes


def mk_wasm_node_func(
    wasm_registry: IWasmRegistry,
    module_name: str,
    entry_point: str,
) -> Callable[[INodeRuntime], Awaitable[None]]:
    wasm_bytes = wasm_registry.get_module(module_name)
    assert wasm_bytes is not None, f"WASM module '{module_name}' not found in registry"

    async def wasm_node(runtime: INodeRuntime) -> None:
        def init_and_run() -> None:
            # Set up WASM environment
            store = wasmer.Store()
            module = wasmer.Module(store, wasm_bytes)
            import_object = wasmer.ImportObject()
            buf_to_str = BufToStr()
            fill_wasm_import_object(store, import_object, buf_to_str, runtime)

            # Create WASM instance
            instance = wasmer.Instance(module, import_object)
            run_fn = getattr(instance.exports, entry_point)

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

    return wasm_node
