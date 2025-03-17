import asyncio
from typing import Dict, Optional
import wasmer  # type: ignore[import-untyped]
from ailets.cons.atyping import INodeRuntime
from ailets.cons.node_runtime_wasm import BufToStr, fill_wasm_import_object


class WasmRegistry:
    def __init__(self):
        self._modules: Dict[str, bytes] = {}

    def register(self, name: str, wasm_bytes: bytes) -> None:
        """Register a WASM module with the given name."""
        self._modules[name] = wasm_bytes

    def get_module(self, name: str) -> Optional[bytes]:
        """Get a WASM module by name."""
        return self._modules.get(name)


async def run_wasm_module(
    wasm_registry: WasmRegistry,
    module_name: str,
    entry_point: str,
    runtime: INodeRuntime,
) -> None:
    """
    Run a WASM module from the registry.
    
    Args:
        wasm_registry: Registry containing WASM modules
        module_name: Name of the module to run
        entry_point: Name of the export function to call
        runtime: Node runtime interface
    """
    wasm_bytes = wasm_registry.get_module(module_name)
    assert wasm_bytes is not None, f"WASM module '{module_name}' not found in registry"

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
