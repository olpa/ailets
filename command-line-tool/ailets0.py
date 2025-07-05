#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import asyncio
import itertools
import json
from io import BytesIO, TextIOWrapper
import sys
import logging
from typing import Any, Awaitable, Callable, Iterator, Literal, Optional, Tuple
import localsetup  # noqa: F401
from ailets.models.well_known import (
    get_model_opts,
    get_wellknown_aliases,
    get_wellknown_models,
)
from ailets.atyping import IKVBuffers
from ailets.cons.util import open_file, save_file
from ailets.cons.dump import dump_environment, load_environment, print_dependency_tree
from ailets.io.sqlitekv import SqliteKV
from ailets.cons.environment import Environment
from ailets.cons.plugin import (
    NodeRegistry,
    hijack_gpt_resp2msg,
    hijack_msg2md,
    hijack_msg2query,
)
from ailets.cons.flow_builder import (
    CmdlinePromptItem,
    instantiate_with_deps,
    media_to_alias,
    media_to_dagops,
    prompt_to_dagops,
    toml_to_env,
    toolspecs_to_alias,
    toolspecs_to_dagops,
    dup_output_to_stdout,
)
from ailets.actor_runtime.node_wasm import WasmRegistry
import re
import os
from urllib.parse import urlparse
import signal
import ailets.cons.minishell as minishell


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="AI Command Line Tool")

    # Required action argument
    parser.add_argument(
        "model",
        help=(
            "The model to run. The best choices are `gpt`, `gemini` or `claude`. "
            "To get the list of models, run the tool with a non-existing model "
            "name 'list'."
        ),
    )

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

    def parse_opt(s: str) -> tuple[str, Any]:
        key, value = s.split("=", 1)
        try:
            value = json.loads(value)
        except json.JSONDecodeError:
            pass
        return key, value

    parser.add_argument(
        "--opt",
        action="append",
        default=[],
        help=(
            "Options in key=value format. "
            "The value is parsed as JSON if possible, otherwise used as string. "
            "Most important keys are 'http.url' and 'llm.model'."
        ),
        metavar="KEY=VALUE",
        type=parse_opt,
    )
    parser.add_argument(
        "--download-to",
        metavar="DIRECTORY",
        default="out",
        help=(
            "Directory to download generated files to. "
            "Is a placeholder for future use."
        ),
    )
    parser.add_argument(
        "--file-system",
        metavar="PATH",
        help=(
            "Path to the virtual file system database in the Python dbm.sqlite3 format"
        ),
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Enable debug logging",
    )
    return parser.parse_args()


def mk_save_env(env: Environment) -> Callable[[BytesIO], Awaitable[None]]:
    async def save_env(stream: BytesIO) -> None:
        tio = TextIOWrapper(stream, encoding="utf-8")
        await dump_environment(env, tio)
        tio.flush()
        tio.detach()

    return save_env


async def coredump(vfs: Optional[IKVBuffers], env: Optional[Environment]) -> None:
    if not env:
        return
    await save_file(vfs, "ailets-core-dump.json", mk_save_env(env))


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
            with open(content, "r") as f:
                file_content = f.read()
            yield from iter_get_prompt(file_content)
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


env: Optional[Environment] = None
vfs: Optional[IKVBuffers] = None


def cleanup() -> None:
    global vfs
    global env
    if vfs:
        if env:
            env.piper.flush_pipes()
        vfs.destroy()
        vfs = None
    if env:
        env.destroy()
        env = None


async def main() -> None:
    global vfs
    global env

    args = parse_args()

    try:
        model_opts = get_model_opts(args.model)
    except KeyError:
        print(f"Model `{args.model}` not found. Available models:")
        print(", ".join(sorted(get_wellknown_models())))
        print("Aliases:")
        aliases = get_wellknown_aliases()
        for alias in sorted(aliases):
            print(f"{alias} -> {aliases[alias]}")
        return
    model = model_opts["ailets.model"]

    # Setup logging
    logging_level = logging.DEBUG if args.debug else logging.INFO
    logging.basicConfig(
        level=logging_level,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    )

    if args.file_system:
        vfs = SqliteKV(args.file_system)

    nodereg = NodeRegistry()
    nodereg.load_plugin("ailets.stdlib", "")
    nodereg.load_plugin(f"ailets.models.{model}", f".{model}")
    for tool in args.tools:
        nodereg.load_plugin(f"ailets.tools.{tool}", f".tool.{tool}")

    prompt = get_prompt(args.prompt)

    if model.startswith("gpt"):
        wasm_registry = WasmRegistry()
        hijack_msg2md(nodereg, wasm_registry)
        hijack_gpt_resp2msg(nodereg, wasm_registry)
        hijack_msg2query(nodereg, wasm_registry)

    if args.load_state:
        with open_file(vfs, args.load_state) as h:
            tio = TextIOWrapper(h, encoding="utf-8")
            env = await load_environment(tio, nodereg)
        toml_to_env(env, model_opts, args.opt, toml=prompt)
        target_node_name = next(
            node_name
            for node_name in env.dagops.get_node_names()
            if node_name.startswith(".messages_to_markdown")
        )

    else:
        env = Environment(nodereg, kv=vfs)
        toml_to_env(env, model_opts, args.opt, toml=prompt)
        tools_prompt = toolspecs_to_dagops(env, args.tools)
        toolspecs_to_alias(env, tools_prompt)
        media_ref_prompt = media_to_dagops(env, prompt)
        media_to_alias(env, media_ref_prompt)
        await prompt_to_dagops(env, prompt=list(tools_prompt) + list(media_ref_prompt))
        model_node_name = instantiate_with_deps(env.dagops, nodereg, f".{model}", {})
        env.dagops.alias(".model_output", model_node_name)

        target_node_name = instantiate_with_deps(
            env.dagops, nodereg, ".messages_to_markdown", {}
        )

    stop_after_node = args.stop_after or target_node_name
    stop_before_node = args.stop_before
    env.processes.resolve_deps()

    print_nodes = {target_node_name, stop_after_node}
    if stop_before_node:
        print_nodes.update(dep.source for dep in env.dagops.iter_deps(stop_before_node))
    dup_output_to_stdout(env, print_nodes)

    if args.dry_run:
        print_dependency_tree(env.dagops, env.processes, target_node_name)
    else:
        # Handle '--one-step', '--stop-before' and '--stop-after'
        node_iter = env.processes.next_node_iter(
            target_node_name, args.one_step, stop_before_node, stop_after_node
        )
        node_iter, node_iter_copy = itertools.tee(node_iter)
        first_node = next(node_iter_copy, None)

        if args.one_step and first_node is not None:
            dup_output_to_stdout(env, {first_node})

        for maybe_built_node in print_nodes:
            if env.processes.is_node_finished(maybe_built_node):
                try:
                    pipe = env.piper.get_existing_pipe(maybe_built_node, "")
                    reader = pipe.get_reader(env.seqno.next_seqno())
                    output = await reader.read(-1)
                    if output:
                        print(output.decode("utf-8"))
                    else:
                        print(
                            f"'{maybe_built_node}' is already built (no output)",
                            file=sys.stderr,
                        )
                except KeyError:
                    print(
                        f"'{maybe_built_node}' is already built (no pipe)",
                        file=sys.stderr,
                    )
                first_node = None
                break

        # If there are nodes to run, prepare and run them
        if first_node is not None:

            # Setup Ctrl+Z handler
            def ctrl_z_handler(signum: int, frame: Any) -> None:
                minishell.MiniShell(env).cmdloop()

            signal.signal(signal.SIGTSTP, ctrl_z_handler)

            # Run nodes
            try:
                await env.processes.run_nodes(node_iter)
            except Exception as e:
                await coredump(vfs, env)
                cleanup()
                raise e

            # Reset SIGTSTP handler back to default
            signal.signal(signal.SIGTSTP, signal.SIG_DFL)

    if args.save_state:
        await save_file(vfs, args.save_state, mk_save_env(env))

    if not args.dry_run:
        output_files = env.kv.listdir("out")
        if len(output_files):
            os.makedirs(args.download_to, exist_ok=True)
        for fname in output_files:

            async def write_out_file(stream: BytesIO) -> None:
                content = env.kv.open(fname, "read").borrow_mut_buffer()
                stream.write(content)

            out_fname = os.path.join(args.download_to, os.path.basename(fname))
            await save_file(vfs, out_fname, write_out_file)

        if errno := env.get_errno():
            await coredump(vfs, env)
            cleanup()
            sys.exit(errno)

    cleanup()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        print("\nCancelled by user")
        asyncio.run(coredump(vfs, env))
        cleanup()
        sys.exit(1)
