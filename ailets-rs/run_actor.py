#!/usr/bin/env python3

import argparse


def parse_name_value(arg: str) -> tuple[str, str]:
    if "=" not in arg:
        raise ValueError(f"Argument '{arg}' must contain '='")
    name, value = arg.split("=", 1)
    return name, value


def parse_arguments() -> tuple[str, dict[str, str]]:
    parser = argparse.ArgumentParser(description="Run a WASM actor")
    parser.add_argument("wasm_path", help="Path to the WASM file")
    parser.add_argument(
        "name_values",
        nargs="*",
        help="""Name-value pairs in format "name=value". Notes:
        - Usually name is empty, so parameter starts with "="
        - If value is empty or "-", reads from stdin or writes to stdout
        - If value starts with "@", reads from or writes to the specified file""",
    )

    args = parser.parse_args()

    # Parse name-value pairs
    name_values = {}
    for arg in args.name_values:
        name, value = parse_name_value(arg)
        name_values[name] = value

    return args.wasm_path, name_values


def main() -> None:
    wasm_path, name_values = parse_arguments()
    print(wasm_path, name_values)


if __name__ == "__main__":
    main()
