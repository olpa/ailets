#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import sys
import localsetup  # noqa: F401
from ailets.cons import (
    mkenv,
    prompt_to_md,
    build_plan_writing_trace,
    load_state_from_trace,
    Environment,
)
from ailets.cons.pipelines import get_func_map
import json
from ailets.cons.nodes.tool_get_user_name import (
    get_spec_for_get_user_name,
    run_get_user_name,
)


def parse_args():
    parser = argparse.ArgumentParser(description="AI Command Line Tool")

    # Required action argument
    parser.add_argument("model", help="The model to run")

    # Optional arguments
    parser.add_argument(
        "--prompt",
        action="append",
        default=[],
        help='Input prompt (default: "-" for stdin). Can be specified multiple times',
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


def get_prompt(prompt_args: list[str]) -> list[str]:
    """Get prompt from arguments or stdin.

    Args:
        prompt_args: List of prompt arguments

    Returns:
        List of prompt strings
    """
    if not prompt_args:
        prompt = ["-"]
    prompt = []
    for prompt_arg in prompt_args:
        if prompt_arg == "-":
            prompt.append(sys.stdin.read())
        else:
            prompt.append(prompt_arg)
    return prompt


def save_state(file_name: str, env: Environment, target_node_name: str):
    plan = env.plan(target_node_name)
    plan_nodes = [env.nodes[name] for name in plan]
    with open(file_name, "w") as f:
        for node in plan_nodes:
            json.dump(node.to_json(), f, indent=2)
            f.write("\n")


def main():
    args = parse_args()
    assert args.model == "gpt4o", "At the moment, only gpt4o is supported"

    env = mkenv()
    env.add_tool("get_user_name", (get_spec_for_get_user_name, run_get_user_name))

    if args.load_state:
        node = load_state_from_trace(env, args.load_state, get_func_map())
    else:
        prompt = get_prompt(args.prompt)
        node = prompt_to_md(env, prompt=prompt, tools=args.tools)

    target_node_name = node.name
    stop_node_name = args.stop_at or target_node_name

    if args.dry_run:
        env.print_dependency_tree(target_node_name)
    else:
        build_plan_writing_trace(env, stop_node_name, one_step=args.one_step)

    if args.save_state:
        save_state(args.save_state, env, target_node_name)


if __name__ == "__main__":
    main()
