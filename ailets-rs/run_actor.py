#!/usr/bin/env python3

import argparse
from dataclasses import dataclass
from typing import Literal, Sequence, cast


@dataclass
class Spec:
    direction: Literal["in", "out"]
    name: str
    value_or_file: str


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


if __name__ == "__main__":
    main()
