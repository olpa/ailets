import asyncio
import logging
import os
import sys

sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from ailets.atyping import Dependency  # noqa: E402
from ailets.cons.environment import Environment  # noqa: E402
from ailets.cons.plugin import NodeRegistry  # noqa: E402
from ailets.cons.dump import print_dependency_tree  # noqa: E402
from ailets.cons.flow_builder import dup_output_to_stdout  # noqa: E402

from copy_actor import copy_actor  # noqa: E402
from stdin_actor import stdin_actor  # noqa: E402


def build_flow(env: Environment) -> None:
    val = env.dagops.add_value_node(
        "(mee too)".encode("utf-8"),
        env.piper,
        env.processes,
        explain="Static text",
    )
    stdin = env.dagops.add_node(
        "stdin",
        stdin_actor,
        [],
        explain="Read from stdin",
    )
    foo = env.dagops.add_node(
        "foo",
        copy_actor,
        [Dependency(stdin.name)],
        explain="Copy",
    )
    bar = env.dagops.add_node(
        "bar",
        copy_actor,
        [Dependency(val.name), Dependency(foo.name)],
        explain="Copy",
    )
    baz = env.dagops.add_node(
        "baz",
        copy_actor,
        [Dependency(bar.name)],
        explain="Copy",
    )

    env.dagops.alias(".end", baz.name)


async def main() -> None:
    node_registry = NodeRegistry()
    env = Environment(node_registry)

    build_flow(env)
    end_node = env.dagops.get_node(".end")
    assert end_node is not None, "End node not defined"

    print_dependency_tree(env.dagops, env.processes, end_node.name)
    dup_output_to_stdout(env, {end_node.name})

    env.processes.resolve_deps()
    node_iter = env.processes.next_node_iter(
        end_node.name,
        flag_one_step=False,
        stop_before=None,
        stop_after=None,
    )
    await env.processes.run_nodes(node_iter)


if __name__ == "__main__":
    level = logging.DEBUG if "--debug" in sys.argv else logging.INFO
    logging.basicConfig(level=level)
    asyncio.run(main())
