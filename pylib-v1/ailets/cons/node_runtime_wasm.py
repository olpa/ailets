import wasmer  # type: ignore[import-untyped]
import base64
import asyncio
import json
import sys
from pydantic import BaseModel
from typing import Dict

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


def fill_wasm_import_object(
    store: wasmer.Store,
    import_object: wasmer.ImportObject,
    buf_to_str: BufToStr,
    runtime: INodeRuntime,
) -> None:
    async def open_read(name_ptr: int) -> int:
        name = buf_to_str.get_string(name_ptr)
        return await runtime.open_read(name)

    async def open_write(name_ptr: int) -> int:
        name = buf_to_str.get_string(name_ptr)
        return await runtime.open_write(name)

    async def aread(fd: int, buffer_ptr: int, count: int) -> int:
        buffer = bytearray(count)
        bytes_read = await runtime.read(fd, buffer, count)
        if bytes_read == -1:
            return -1
        buf_view = buf_to_str.get_view()
        end = buffer_ptr + bytes_read
        buf_view[buffer_ptr:end] = buffer[:bytes_read]
        return bytes_read

    async def awrite(fd: int, buffer_ptr: int, count: int) -> int:
        buf_view = buf_to_str.get_view()
        end = buffer_ptr + count
        buffer = bytes(buf_view[buffer_ptr:end])
        return await runtime.write(fd, buffer, count)

    async def aclose(fd: int) -> int:
        return await runtime.close(fd)

    async def dag_instantiate_with_deps(workflow_ptr: int, deps_ptr: int) -> int:
        workflow = buf_to_str.get_string(workflow_ptr)
        deps = buf_to_str.get_string(deps_ptr)

        try:
            deps_dict = json.loads(deps)
        except Exception as e:
            print(
                f"instantiate_with_deps: Error parsing '{workflow}'s input deps: {e}",
                file=sys.stderr,
            )
            return -1

        try:

            class DepsModel(BaseModel):
                deps: Dict[str, int]

            validated_deps = DepsModel(deps=deps_dict).deps
        except Exception as e:
            print(
                f"instantiate_with_deps: Error validating '{workflow}'s input deps: "
                f"{e}",
                file=sys.stderr,
            )
            return -1

        try:
            handle = runtime.dagops().v2_instantiate_with_deps(workflow, validated_deps)
            return handle
        except Exception as e:
            print(
                f"instantiate_with_deps: Error instantiating workflow {workflow} "
                f"with deps {validated_deps}: {e}",
                file=sys.stderr,
            )
            return -1

    async def dag_value_node(value_ptr: int, explain_ptr: int) -> int:
        value_str = buf_to_str.get_string(value_ptr)
        explain_str = buf_to_str.get_string(explain_ptr)
        try:
            value = base64.b64decode(value_str)
        except Exception as e:
            print(
                f"value_node: Error decoding value for '{explain_str}': {e}",
                file=sys.stderr,
            )
            return -1

        try:
            handle = runtime.dagops().v2_add_value_node(value, explain_str)
            return handle
        except Exception as e:
            print(
                f"value_node: Error adding value node for '{explain_str}': {e}",
                file=sys.stderr,
            )
            return -1

    async def dag_alias(alias_ptr: int, node_handle: int) -> int:
        alias = buf_to_str.get_string(alias_ptr)
        try:
            handle = runtime.dagops().v2_alias(alias, node_handle)
            return handle
        except Exception as e:
            print(
                f"alias: Error adding alias '{alias}' to node {node_handle}: {e}",
                file=sys.stderr,
            )
            return -1

    async def dag_detach_from_alias(alias_ptr: int) -> int:
        alias = buf_to_str.get_string(alias_ptr)
        try:
            runtime.dagops().detach_from_alias(alias)
            return 0
        except Exception as e:
            print(
                f"detach_from_alias: Error detaching from alias '{alias}': {e}",
                file=sys.stderr,
            )
            return -1

    def sync_open_read(name_ptr: int) -> int:
        return asyncio.run(open_read(name_ptr))

    def sync_open_write(name_ptr: int) -> int:
        return asyncio.run(open_write(name_ptr))

    def sync_aread(fd: int, buffer_ptr: int, count: int) -> int:
        return asyncio.run(aread(fd, buffer_ptr, count))

    def sync_awrite(fd: int, buffer_ptr: int, count: int) -> int:
        return asyncio.run(awrite(fd, buffer_ptr, count))

    def sync_aclose(fd: int) -> int:
        return asyncio.run(aclose(fd))

    def sync_get_errno() -> int:
        return runtime.get_errno()

    def sync_dag_instantiate_with_deps(workflow_ptr: int, deps_ptr: int) -> int:
        return asyncio.run(dag_instantiate_with_deps(workflow_ptr, deps_ptr))

    def sync_dag_value_node(value_ptr: int, explain_ptr: int) -> int:
        return asyncio.run(dag_value_node(value_ptr, explain_ptr))

    def sync_dag_alias(alias_ptr: int, node_handle: int) -> int:
        return asyncio.run(dag_alias(alias_ptr, node_handle))

    def sync_dag_detach_from_alias(alias_ptr: int) -> int:
        return asyncio.run(dag_detach_from_alias(alias_ptr))

    # Register functions with WASM
    import_object.register(
        "",
        {
            "open_read": wasmer.Function(store, sync_open_read),
            "open_write": wasmer.Function(store, sync_open_write),
            "aread": wasmer.Function(store, sync_aread),
            "awrite": wasmer.Function(store, sync_awrite),
            "aclose": wasmer.Function(store, sync_aclose),
            "get_errno": wasmer.Function(store, sync_get_errno),
            "dag_instantiate_with_deps": wasmer.Function(
                store, sync_dag_instantiate_with_deps
            ),
            "dag_value_node": wasmer.Function(store, sync_dag_value_node),
            "dag_alias": wasmer.Function(store, sync_dag_alias),
            "dag_detach_from_alias": wasmer.Function(store, sync_dag_detach_from_alias),
        },
    )
