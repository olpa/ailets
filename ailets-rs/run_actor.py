#!/usr/bin/env python3

import argparse
from dataclasses import dataclass
import io
import sys
from typing import Literal, Optional, Protocol, Sequence, cast


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


def main() -> None:
    wasm_path, specs = parse_arguments()
    print(wasm_path, specs)


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
        return [
            spec
            for spec in self.specs
            if spec.direction == direction and spec.name == name
        ]

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

    def read(self, fd: int, buffer: bytearray, count: int) -> int:
        stream = self.streams[fd]
        if stream is None:
            raise ValueError(f"Stream {fd} is not open")

        bytes = stream.read(count)
        if bytes is None:
            return 0
        buffer[: len(bytes)] = bytes
        return len(bytes)

    def write(self, fd: int, buffer: bytes, count: int) -> int:
        stream = self.streams[fd]
        if stream is None:
            raise ValueError(f"Stream {fd} is not open")
        return stream.write(buffer[:count])

    def close(self, fd: int) -> None:
        stream = self.streams[fd]
        if stream is None:
            raise ValueError(f"Stream {fd} is not open")
        if stream is not sys.stdin.buffer and stream is not sys.stdout.buffer:
            stream.close()
        self.streams[fd] = None


# ----


if __name__ == "__main__":
    main()
