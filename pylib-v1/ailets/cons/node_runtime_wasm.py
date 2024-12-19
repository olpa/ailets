from typing import Any

import wasmer  # type: ignore[import-untyped]

from .atyping import INodeRuntime


class BufToStr:
    def __init__(self) -> None:
        self.memory: wasmer.Memory | None = None

    def set_memory(self, memory: wasmer.Memory) -> None:
        self.memory = memory

    def get_string(self, ptr: int) -> str:
        if self.memory is None:
            raise ValueError("Memory is not set")
        buf = memoryview(self.memory.buffer)
        end = ptr
        while buf[end] != 0:
            end += 1
        str_bytes = bytes(buf[ptr:end])
        return str_bytes.decode()

    def get_view(self) -> memoryview:
        if self.memory is None:
            raise ValueError("Memory is not set")
        return memoryview(self.memory.buffer)


def create_wasm_runtime(
    store: wasmer.Store,
    import_object: wasmer.ImportObject,
    buf_to_str: BufToStr,
    runtime: INodeRuntime,
) -> None:
    async def n_of_streams(name_ptr: int) -> int:
        name = buf_to_str.get_string(name_ptr)
        return runtime.n_of_streams(name)

    async def open_read(name_ptr: int, index: int) -> int:
        name = buf_to_str.get_string(name_ptr)
        return await runtime.open_read(name, index)

    async def open_write(name_ptr: int) -> int:
        name = buf_to_str.get_string(name_ptr)
        return await runtime.open_write(name)

    async def aread(fd: int, buffer_ptr: int, count: int) -> int:
        buffer = bytearray(count)
        bytes_read = await runtime.read(fd, buffer, count)
        buf_view = buf_to_str.get_view()
        end = buffer_ptr + bytes_read
        buf_view[buffer_ptr:end] = buffer[:bytes_read]
        return bytes_read

    async def awrite(fd: int, buffer_ptr: int, count: int) -> int:
        buf_view = buf_to_str.get_view()
        end = buffer_ptr + count
        buffer = bytes(buf_view[buffer_ptr:end])
        return await runtime.write(fd, buffer, count)

    async def aclose(fd: int) -> None:
        await runtime.close(fd)

    # Convert async functions to sync for WASM compatibility
    def make_sync(f: Any) -> Any:
        def wrapper(*args: Any) -> Any:
            import asyncio

            return asyncio.run(f(*args))

        return wrapper

    # Register functions with WASM
    import_object.register(
        "",
        {
            "n_of_streams": wasmer.Function(store, make_sync(n_of_streams)),
            "open_read": wasmer.Function(store, make_sync(open_read)),
            "open_write": wasmer.Function(store, make_sync(open_write)),
            "aread": wasmer.Function(store, make_sync(aread)),
            "awrite": wasmer.Function(store, make_sync(awrite)),
            "aclose": wasmer.Function(store, make_sync(aclose)),
        },
    )
