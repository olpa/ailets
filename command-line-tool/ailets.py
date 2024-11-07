#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import argparse
import setup
from cons import mkenv, prompt_to_md


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


def main():
    args = parse_args()
    assert args.model == "gpt4o", "At the moment, only gpt4o is supported"

    env = mkenv()
    node = prompt_to_md(env)
    env.print_dependency_tree(node.name)


if __name__ == "__main__":
    main()
