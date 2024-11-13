#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import sys
import localsetup  # noqa: F401
from typing import Union, Tuple
from ailets.cons.cons import Environment
from ailets.cons import (
    prompt_to_md,
)
from ailets.cons.pipelines import get_func_map
from ailets.cons.nodes.tool_get_user_name import (
    get_spec_for_get_user_name,
    run_get_user_name,
)
import re
import base64
import os
from urllib.parse import urlparse


def parse_args():
    parser = argparse.ArgumentParser(description="AI Command Line Tool")

    # Required action argument
    parser.add_argument("model", help="The model to run")

    # Optional arguments
    parser.add_argument(
        "--prompt",
        action="append",
        default=[],
        help="""Input prompt. Can be specified multiple times. Formats:\\\\

            - text: regular text\\\\
            - "-": read from stdin\\\\
            - "@path/to/file": local file with auto-detected type\\\\
            - "@{type}path/to/file": local file with explicit type\\\\
            - "@http://...": URL with auto-detected type\\\\
            - "@{type}http://...": URL with explicit type\\\\

            Supported types: text, image_url""",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Perform a dry run without making changes",
    )
    parser.add_argument(
        "--save-state", metavar="FILE", help="Save state to specified file"
    )
    parser.add_argument(
        "--load-state", metavar="FILE", help="Load state from specified file"
    )
    parser.add_argument("--one-step", action="store_true", help="Execute only one step")
    parser.add_argument("--stop-at", help="Stop execution at specified point")
    parser.add_argument(
        "--tool",
        nargs="+",
        dest="tools",
        default=[],
        help="List of tools to use (e.g. get_user_name)",
    )
    return parser.parse_args()


def is_url(s: str) -> bool:
    """Check if string is a valid URL."""
    try:
        result = urlparse(s)
        return all([result.scheme, result.netloc])
    except ValueError:
        return False


def guess_content_type(content: str) -> str:
    """Guess content type from content or URL."""
    # Common image extensions
    image_extensions = {".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp"}
    # Text extensions
    text_extensions = {".txt", ".md", ".text"}

    ext = os.path.splitext(urlparse(content).path)[1].lower()
    if ext in image_extensions:
        return "image_url"
    if ext in text_extensions:
        return "text"
    raise ValueError(f"Could not determine content type for: {content}")


def get_prompt(prompt_args: list[str]) -> list[Union[str, Tuple[str, str]]]:
    """Get prompt from arguments or stdin.

    Args:
        prompt_args: List of prompt arguments

    Returns:
        List of prompts. Each prompt can be either:
            - str: treated as a regular prompt
            - tuple[str, str]: (text, type) for typed content like images
    """
    prompt: list[Union[str, Tuple[str, str]]] = []
    if not prompt_args:
        prompt = ["-"]

    for prompt_arg in prompt_args:
        if prompt_arg == "-":
            prompt.append(sys.stdin.read())
            continue

        if not prompt_arg.startswith("@"):
            prompt.append(prompt_arg)
            continue

        # Parse @{type}content format
        match = re.match(r"^@({\w+})?(.+)$", prompt_arg)
        if not match:
            raise ValueError(f"Invalid format for typed content: {prompt_arg}")

        content_type, content = match.groups()

        # If type not specified, try to guess it
        if content_type is None:
            content_type = guess_content_type(content)

        # Handle URLs vs files
        if not is_url(content):
            # Read file and convert to data URL
            with open(content, "rb") as f:
                file_content = f.read()
            content = (
                f"data:image/jpeg;base64,{base64.b64encode(file_content).decode()}"
            )

        supported_content_types = ["text", "image_url"]
        content_type_stripped = content_type.strip("{}")
        error_msg = (
            f"Unknown content type: {content_type_stripped}, "
            f"expected: {supported_content_types}"
        )
        assert content_type_stripped in supported_content_types, error_msg

        prompt.append((content, content_type))

    return prompt


def main():
    args = parse_args()
    assert args.model == "gpt4o", "At the moment, only gpt4o is supported"

    if args.load_state:
        with open(args.load_state, "r") as f:
            env = Environment.from_json(f, get_func_map())
        env.add_tool("get_user_name", (get_spec_for_get_user_name, run_get_user_name))
        node = env.find_final_node()
    else:
        env = Environment()
        env.add_tool("get_user_name", (get_spec_for_get_user_name, run_get_user_name))
        prompt = get_prompt(args.prompt)
        node = prompt_to_md(env, prompt=prompt, tools=args.tools)

    target_node_name = node.name
    stop_node_name = args.stop_at or target_node_name

    if args.dry_run:
        env.print_dependency_tree(target_node_name)
    else:
        env.build_target(stop_node_name, one_step=args.one_step)

    if args.save_state:
        with open(args.save_state, "w") as f:
            env.to_json(f)


if __name__ == "__main__":
    main()
