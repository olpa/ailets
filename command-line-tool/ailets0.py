#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import sys
import localsetup  # noqa: F401
from typing import Iterator, Optional, Tuple
from ailets.cons.cons import Environment
from ailets.cons.plugin import NodeRegistry
from ailets.cons.pipelines import (
    CmdlinePromptItem,
    instantiate_with_deps,
    prompt_to_env,
    toml_to_env,
    toolspecs_to_env,
)
import re
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
    parser.add_argument(
        "--download-to",
        metavar="DIRECTORY",
        default="./out",
        help=(
            "Directory to download generated files to. "
            "Is a placeholder for future use."
        ),
    )
    return parser.parse_args()


def is_url(s: str) -> bool:
    """Check if string is a valid URL."""
    try:
        result = urlparse(s)
        return all([result.scheme, result.netloc])
    except ValueError:
        return False


def guess_content_type(content: str) -> Tuple[str, str]:
    # Common image extensions and their media types
    image_extensions = {
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".png": "image/png",
        ".gif": "image/gif",
        ".webp": "image/webp",
        ".bmp": "image/bmp",
    }
    # Text extensions and their media types
    text_extensions = {
        ".txt": "text/plain",
        ".md": "text/markdown",
        ".text": "text/plain",
    }

    ext = os.path.splitext(urlparse(content).path)[1].lower()

    if ext in image_extensions:
        return "image_url", image_extensions[ext]

    if ext in text_extensions:
        return "text", text_extensions[ext]

    raise ValueError(f"Could not determine content type for: {content}")


def get_prompt(prompt_args: list[str]) -> list[CmdlinePromptItem]:
    """Get prompt from arguments or stdin.

    Args:
        prompt_args: List of prompt arguments

    Returns:
        List of prompts. Each prompt can be either:
            - str: treated as a regular prompt
            - tuple[str, str]: (text, type) for typed content like images
    """

    def iter_get_prompt(arg: str) -> Iterator[CmdlinePromptItem]:
        if not arg:
            return

        if arg == "-":
            s = sys.stdin.read()
            yield from iter_get_prompt(s)
            return

        if not arg.startswith("@"):

            def split_text_toml_and_text(
                text: str,
            ) -> Tuple[Optional[str], Optional[str]]:
                a = text.split("---\n", 1)
                if len(a) == 2:
                    return a[0].strip(), a[1].strip()
                if not text.startswith("```toml\n"):
                    return (None, text)
                a = text[7:].split("```", 1)
                if len(a) == 2:
                    return a[0].strip(), a[1].strip()
                return (a[0].strip(), None)

            toml, text = split_text_toml_and_text(arg)
            if toml:
                yield CmdlinePromptItem(toml.encode("utf-8"), "toml")
            if text:
                yield CmdlinePromptItem(text.encode("utf-8"), "text")
            return

        # Parse @{type}content format
        match = re.match(r"^@({\w+})?(.+)$", arg)
        if not match:
            raise ValueError(f"Invalid format for typed content: {arg}")

        media_type, content = match.groups()
        if media_type:
            media_type = media_type[1:-1]  # Remove curly braces

        # If type not specified, try to guess it
        if media_type is None:
            content_type, media_type = guess_content_type(content)
        else:
            assert "/" in media_type, "Media type must contain a slash"
            content_type = media_type.split("/")[0]
            assert content_type in ["image", "text"], "Unknown content type"

        if content_type == "text":
            yield from iter_get_prompt(content)
            return

        supported_content_types = ["text", "image"]
        error_msg = (
            f"Unknown content type: {content_type}, "
            f"expected: {supported_content_types}"
        )
        assert content_type in supported_content_types, error_msg

        if is_url(content) or content.startswith("data:"):
            content_type = f"{content_type}_url"
        yield CmdlinePromptItem(content.encode("utf-8"), content_type)

    items = [p for prompt_arg in prompt_args for p in iter_get_prompt(prompt_arg)]
    if not len(items):
        items = [CmdlinePromptItem(b"Hello!", "text")]
    return items


def main():
    args = parse_args()
    assert args.model in [
        "gpt4o",
        "dalle",
    ], "At the moment, only gpt4o and dalle are supported"

    nodereg = NodeRegistry()
    nodereg.load_plugin("ailets.stdlib", "")
    nodereg.load_plugin(f"ailets.models.{args.model}", f".{args.model}")
    for tool in args.tools:
        nodereg.load_plugin(f"ailets.tools.{tool}", f".tool.{tool}")

    prompt = get_prompt(args.prompt)

    if args.load_state:
        with open(args.load_state, "r") as f:
            env = Environment.from_json(f, nodereg)
        toml_to_env(env, toml=prompt)
        target_node_name = next(
            node_name
            for node_name in env.nodes.keys()
            if node_name.startswith(".stdout")
        )

    else:
        env = Environment()
        toml_to_env(env, toml=prompt)
        toolspecs_to_env(env, nodereg, args.tools)
        prompt_to_env(env, prompt=prompt)

        chat_node_name = instantiate_with_deps(env, nodereg, ".prompt_to_messages", {})
        env.alias(".chat_messages", chat_node_name)

        model_node_name = instantiate_with_deps(env, nodereg, f".{args.model}", {})
        env.alias(".model_output", model_node_name)

        resolve = {
            ".prompt_to_messages": chat_node_name,
        }
        target_node_name = instantiate_with_deps(env, nodereg, ".stdout", resolve)

    stop_node_name = args.stop_at or target_node_name

    if args.dry_run:
        env.print_dependency_tree(target_node_name)
    else:
        env.build_target(nodereg, stop_node_name, one_step=args.one_step)

    if args.save_state:
        with open(args.save_state, "w") as f:
            env.to_json(f)

    if not args.dry_run:
        fs_output_streams = env.get_fs_output_streams()
        if len(fs_output_streams):
            os.makedirs(args.download_to, exist_ok=True)
        for stream in fs_output_streams:
            with open(
                os.path.join(args.download_to, os.path.basename(stream.stream_name)),
                "wb",
            ) as f:
                f.write(stream.content.getvalue())


if __name__ == "__main__":
    main()
