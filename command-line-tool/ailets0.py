#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import asyncio
import sys
import logging
import localsetup  # noqa: F401
from ailets.cons.dump import dump_environment, load_environment, print_dependency_tree
from typing import Any, Iterator, Literal, Optional, Tuple
from ailets.cons.environment import Environment
from ailets.cons.plugin import NodeRegistry, hijack_gpt_resp2msg, hijack_msg2md
from ailets.cons.pipelines import (
    CmdlinePromptItem,
    instantiate_with_deps,
    prompt_to_dagops,
    toml_to_env,
    toolspecs_to_dagops,
)
import re
import os
from urllib.parse import urlparse
import signal
import ailets.cons.minishell as minishell


def parse_args() -> argparse.Namespace:
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

            Supported types: `text/*`, `image/*`

            See also full documentation at
            https://github.com/ailets/ailets/docs/command-line-tool.md
            how to use system prompt and TOML configuration""",
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
    parser.add_argument("--stop-after", help="Stop execution after specified point")
    parser.add_argument("--stop-before", help="Stop execution before specified point")
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
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Enable debug logging",
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
    image_extensions = {
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".png": "image/png",
        ".gif": "image/gif",
        ".webp": "image/webp",
        ".bmp": "image/bmp",
    }
    text_extensions = {
        ".txt": "text/plain",
        ".md": "text/markdown",
        ".text": "text/plain",
    }

    ext = os.path.splitext(urlparse(content).path)[1].lower()

    if ext in image_extensions:
        return image_extensions[ext]

    if ext in text_extensions:
        return text_extensions[ext]

    raise ValueError(f"Could not determine content type for: {content}")


def get_prompt(prompt_args: list[str]) -> list[CmdlinePromptItem]:
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
                yield CmdlinePromptItem(toml, "toml")
            if text:
                yield CmdlinePromptItem(text, "text", toml=toml)
            return

        # Parse @{type}content format
        match = re.match(r"^@({[a-zA-Z0-9_/-]+})?(.+)$", arg)
        if not match:
            raise ValueError(f"Invalid format for typed content: {arg}")

        content_type, content = match.groups()
        if content_type:
            content_type = content_type[1:-1]  # Remove curly braces

        # If type not specified, try to guess it
        if content_type is None:
            content_type = guess_content_type(content)

        assert "/" in content_type, f"Content type must contain a slash: {content_type}"
        base_content_type = content_type.split("/")[0]
        assert base_content_type in [
            "image",
            "text",
        ], f"Unknown content type: {base_content_type}"

        if base_content_type == "text":
            yield from iter_get_prompt(content)
            return

        supported_content_types = ["text", "image"]
        error_msg = (
            f"Unknown content type: {base_content_type}, "
            f"expected: {supported_content_types}"
        )
        assert base_content_type in supported_content_types, error_msg

        location: Literal["url", "file"] = (
            "url" if is_url(content) or content.startswith("data:") else "file"
        )
        yield CmdlinePromptItem(content, location, content_type)

    items = [p for prompt_arg in prompt_args for p in iter_get_prompt(prompt_arg)]
    if not len(items):
        items = [CmdlinePromptItem("Hello!", "text")]
    return items


async def main() -> None:
    args = parse_args()
    assert args.model in [
        "gpt4o",
        "dalle",
    ], "At the moment, only gpt4o and dalle are supported"

    # Setup logging
    logging_level = logging.DEBUG if args.debug else logging.INFO
    logging.basicConfig(
        level=logging_level,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )

    nodereg = NodeRegistry()
    nodereg.load_plugin("ailets.stdlib", "")
    nodereg.load_plugin(f"ailets.models.{args.model}", f".{args.model}")
    for tool in args.tools:
        nodereg.load_plugin(f"ailets.tools.{tool}", f".tool.{tool}")

    if args.model == "gpt4o":
        hijack_msg2md(nodereg)
        hijack_gpt_resp2msg(nodereg)

    prompt = get_prompt(args.prompt)

    if args.load_state:
        with open(args.load_state, "r") as f:
            env = await load_environment(f, nodereg)
        toml_to_env(env, toml=prompt)
        target_node_name = next(
            node_name
            for node_name in env.dagops.get_node_names()
            if node_name.startswith(".stdout")
        )

    else:
        env = Environment(nodereg)
        toml_to_env(env, toml=prompt)
        toolspecs_to_dagops(env, args.tools)
        await prompt_to_dagops(env, prompt=prompt)

        chat_node_name = instantiate_with_deps(
            env.dagops, nodereg, ".prompt_to_messages", {}
        )
        env.dagops.alias(".chat_messages", chat_node_name)

        model_node_name = instantiate_with_deps(
            env.dagops, nodereg, f".{args.model}", {}
        )
        env.dagops.alias(".model_output", model_node_name)

        resolve = {
            ".prompt_to_messages": chat_node_name,
        }
        target_node_name = instantiate_with_deps(
            env.dagops, nodereg, ".stdout", resolve
        )

    stop_after_node = args.stop_after or target_node_name
    stop_before_node = args.stop_before
    env.processes.resolve_deps()

    if args.dry_run:
        print_dependency_tree(env.dagops, env.processes, target_node_name)
    else:
        # Setup Ctrl+Z handler
        def ctrl_z_handler(signum: int, frame: Any) -> None:
            minishell.MiniShell(env).cmdloop()

        signal.signal(signal.SIGTSTP, ctrl_z_handler)

        node_iter = env.processes.next_node_iter(
            target_node_name, args.one_step, stop_before_node, stop_after_node
        )
        await env.processes.run_nodes(node_iter)
        # Reset SIGTSTP handler back to default
        signal.signal(signal.SIGTSTP, signal.SIG_DFL)

    if args.save_state:
        with open(args.save_state, "w") as f:
            await dump_environment(env, f)

    if not args.dry_run:
        fs_output_streams = env.streams.get_fs_output_streams()
        if len(fs_output_streams):
            os.makedirs(args.download_to, exist_ok=True)
        for stream in fs_output_streams:
            name = os.path.basename(stream.get_name() or "None")
            with open(os.path.join(args.download_to, name), "wb") as f:
                content = await stream.read(0, -1)
                f.write(content)


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\nCancelled by user")
        sys.exit(1)
