import asyncio
import os
import sys

sys.path.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from ailets.cons.environment import Environment  # noqa: E402
from ailets.cons.plugin import NodeRegistry  # noqa: E402
from ailets.cons.dump import print_dependency_tree  # noqa: E402


def build_flow(env: Environment) -> None:
    hw = env.dagops.add_value_node(
        "Hello world!\n".encode("utf-8"),
        env.piper,
        env.processes,
        explain="Static text",
    )

    env.dagops.alias(".end", hw.name)


async def main() -> None:
    node_registry = NodeRegistry()
    env = Environment(node_registry)

    build_flow(env)

    print_dependency_tree(env.dagops, env.processes, ".end")


if __name__ == "__main__":
    asyncio.run(main())
