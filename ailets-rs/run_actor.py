#!/usr/bin/env python3

import argparse
from dataclasses import dataclass
import io
import base64
import sys
from typing import Literal, Optional, Protocol, Sequence, cast
import wasmer  # type: ignore[import-untyped]


@dataclass
class Spec:
    direction: Literal["in", "out"]
    name: str
    value_or_file: str


# ----


def parse_name_value(arg: str) -> Spec:
    if ":" not in arg:
        raise ValueError(f"Argument '{arg}' must contain ':' to specify direction")
    direction_part, rest = arg.split(":", 1)

    if direction_part not in ["in", "out"]:
        raise ValueError(f"Direction must be 'in' or 'out', got '{direction_part}'")
    direction_part = cast(Literal["in", "out"], direction_part)

    if "=" not in rest:
        raise ValueError(
            f"Argument '{rest}' must contain '=' to specify name and value"
        )
    name, value = rest.split("=", 1)

    return Spec(direction=direction_part, name=name, value_or_file=value)


def parse_arguments() -> tuple[str, Sequence[Spec]]:
    parser = argparse.ArgumentParser(description="Run a WASM actor")
    parser.add_argument("wasm_path", help="Path to the WASM file")
    parser.add_argument(
        "name_values",
        nargs="*",
        help="""Name-value pairs in format "inout:name=value". Notes:
        - "inout" is either "in" or "out"
        - If value is "-", read from stdin or write to stdout
        - If value starts with "@", read from or write to the specified file""",
    )

    args = parser.parse_args()

    # Parse name-value pairs
    specs = []
    for arg in args.name_values:
        specs.append(parse_name_value(arg))

    return args.wasm_path, specs


# ----


class IStream(Protocol):
    def read(self, count: int) -> bytes | None: ...

    def write(self, buffer: bytes) -> int: ...

    def close(self) -> None: ...


class NodeRuntime:
    def __init__(self, specs: Sequence[Spec]) -> None:
        self.specs = specs
        self.streams: list[Optional[IStream]] = []

    def _collect_streams(
        self, direction: Literal["in", "out"], name: str
    ) -> Sequence[Spec]:
        found = [
            spec
            for spec in self.specs
            if spec.direction == direction and spec.name == name
        ]
        if len(found) or name != "":
            return found
        return [Spec(direction=direction, name="", value_or_file="-")]

    def n_of_streams(self, stream_name: str) -> int:
        return len(self._collect_streams("in", stream_name))

    def open_read(self, stream_name: str, index: int) -> int:
        specs = self._collect_streams("in", stream_name)
        if index < 0 or index >= len(specs):
            raise ValueError(f"No stream '{stream_name}' with index {index}")

        vof = specs[index].value_or_file
        if vof == "-":
            self.streams.append(sys.stdin.buffer)
        elif vof.startswith("@"):
            self.streams.append(open(vof[1:], "rb"))
        else:
            self.streams.append(io.BytesIO(vof.encode()))

        return len(self.streams) - 1

    def open_write(self, stream_name: str) -> int:
        specs = self._collect_streams("out", stream_name)
        if not specs:
            raise ValueError(f"No output stream '{stream_name}'")

        vof = specs[0].value_or_file
        if vof == "-":
            self.streams.append(sys.stdout.buffer)
        elif vof.startswith("@"):
            self.streams.append(open(vof[1:], "wb"))
        else:
            self.streams.append(io.BytesIO())

        return len(self.streams) - 1

    def aread(self, fd: int, buffer: memoryview, ptr: int, count: int) -> int:
        stream = self.streams[fd]
        if stream is None:
            raise ValueError(f"Stream {fd} is not open")

        bytes = stream.read(count)
        if bytes is None:
            return 0
        end = ptr + len(bytes)
        buffer[ptr:end] = bytes
        return end - ptr

    def awrite(self, fd: int, buffer: memoryview, ptr: int, count: int) -> int:
        stream = self.streams[fd]
        if stream is None:
            raise ValueError(f"Stream {fd} is not open")
        end = ptr + count
        return stream.write(buffer[ptr:end])

    def aclose(self, fd: int) -> int:
        stream = self.streams[fd]
        if stream is None:
            raise ValueError(f"Stream {fd} is not open")
        if stream is not sys.stdin.buffer and stream is not sys.stdout.buffer:
            stream.close()
        self.streams[fd] = None
        return 0

    def dag_instantiate_with_deps(self, workflow: str, deps: str) -> int:
        handle = len(workflow)
        print(
            f"dag_instantiate_with_deps: workflow: {workflow}, deps: {deps} -> {handle}"
        )
        return handle

    def dag_value_node(self, value: str, explain: str) -> int:
        try:
            value = base64.b64decode(value).decode("utf-8")
        except Exception as e:
            print(f"dag_value_node: Error decoding value: {e}")
            return -1
        handle = len(value)
        print(f"dag_value_node: value: {value}, explain: {explain} -> {handle}")
        return handle

    def dag_alias(self, alias: str, node_handle: int) -> int:
        handle = len(alias)
        print(f"dag_alias: alias: {alias}, node_handle: {node_handle} -> {handle}")
        return handle

    def dag_detach_from_alias(self, alias: str) -> int:
        print(f"dag_detach_from_alias: alias: {alias}")
        return 0


class BufToStr:
    def __init__(self) -> None:
        self.memory: Optional[wasmer.Memory] = None

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


def register_node_runtime(
    store: wasmer.Store,
    import_object: wasmer.ImportObject,
    buf_to_str: BufToStr,
    nr: NodeRuntime,
) -> None:

    def n_of_streams(name_ptr: int) -> int:
        name = buf_to_str.get_string(name_ptr)
        return nr.n_of_streams(name)

    def open_read(name_ptr: int, index: int) -> int:
        name = buf_to_str.get_string(name_ptr)
        return nr.open_read(name, index)

    def open_write(name_ptr: int) -> int:
        name = buf_to_str.get_string(name_ptr)
        return nr.open_write(name)

    def aread(fd: int, buffer_ptr: int, count: int) -> int:
        return nr.aread(fd, buf_to_str.get_view(), buffer_ptr, count)

    def awrite(fd: int, buffer_ptr: int, count: int) -> int:
        return nr.awrite(fd, buf_to_str.get_view(), buffer_ptr, count)

    def aclose(fd: int) -> int:
        return nr.aclose(fd)

    def dag_instantiate_with_deps(workflow: int, deps: int) -> int:
        return nr.dag_instantiate_with_deps(
            buf_to_str.get_string(workflow),
            buf_to_str.get_string(deps),
        )

    def dag_value_node(value: int, explain: int) -> int:
        return nr.dag_value_node(
            buf_to_str.get_string(value),
            buf_to_str.get_string(explain),
        )

    def dag_alias(alias: int, node_handle: int) -> int:
        return nr.dag_alias(
            buf_to_str.get_string(alias),
            node_handle,
        )

    def dag_detach_from_alias(alias: int) -> int:
        return nr.dag_detach_from_alias(buf_to_str.get_string(alias))

    import_object.register(
        "",
        {
            "n_of_streams": wasmer.Function(store, n_of_streams),
            "open_read": wasmer.Function(store, open_read),
            "open_write": wasmer.Function(store, open_write),
            "aread": wasmer.Function(store, aread),
            "awrite": wasmer.Function(store, awrite),
            "aclose": wasmer.Function(store, aclose),
            "dag_instantiate_with_deps": wasmer.Function(
                store, dag_instantiate_with_deps
            ),
            "dag_value_node": wasmer.Function(store, dag_value_node),
            "dag_alias": wasmer.Function(store, dag_alias),
            "dag_detach_from_alias": wasmer.Function(store, dag_detach_from_alias),
        },
    )


# ----


def main() -> None:
    wasm_path, specs = parse_arguments()
    nr = NodeRuntime(specs)

    with open(wasm_path, "rb") as f:
        wasm_bytes = f.read()
    store = wasmer.Store()
    module = wasmer.Module(store, wasm_bytes)
    import_object = wasmer.ImportObject()
    buf_to_str = BufToStr()
    register_node_runtime(store, import_object, buf_to_str, nr)

    instance = wasmer.Instance(module, import_object)

    callable_exports = [
        (name, export) for name, export in instance.exports if callable(export)
    ]
    export_names = [name for name, _ in callable_exports]
    assert (
        len(callable_exports) == 1
    ), f"Expected 1 export, got {len(callable_exports)}: {export_names}"
    run_fn = callable_exports[0][1]

    memory = instance.exports.memory
    assert isinstance(memory, wasmer.Memory), "Memory is not a Memory"
    buf_to_str.set_memory(memory)

    run_fn()


if __name__ == "__main__":
    main()
