#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import sys
import setup  # noqa: F401
from cons import (
    mkenv,
    prompt_to_md,
    build_plan_writing_trace,
    load_state_from_trace,
    Environment,
)
from cons.pipelines import get_func_map
import json


def parse_args():
    parser = argparse.ArgumentParser(description="AI Command Line Tool")

    # Required action argument
    parser.add_argument("model", help="The model to run")

    # Optional arguments
    parser.add_argument(
        "--prompt", default="-", help='Input prompt (default: "-" for stdin)'
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

    return parser.parse_args()


def get_prompt(prompt_arg: str) -> str:
    if prompt_arg == "-":
        return sys.stdin.read()
    return prompt_arg


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

    if args.load_state:
        node = load_state_from_trace(env, args.load_state, get_func_map())
    else:
        prompt = get_prompt(args.prompt)
        node = prompt_to_md(env, prompt)

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
