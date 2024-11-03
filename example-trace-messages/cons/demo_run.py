import os
import json
from cons import Environment
from typing import Dict, Callable, Any


def dump_nodes(nodes: list, path: str) -> None:
    """Dump specified nodes to a JSON file."""
    with open(path, "w") as f:
        for node in nodes:
            json.dump(node.to_json(), f, indent=2)
            f.write("\n")


def build_plan_writing_trace(env: Environment, target: str, trace_dir: str) -> None:
    """Build nodes in order, saving state after each build."""
    plan = env.plan(target)
    plan_nodes = [env.nodes[name] for name in plan]

    # Initial state - dump plan only if all nodes are dirty
    if all(node.dirty for node in plan_nodes):
        os.makedirs(trace_dir, exist_ok=True)
        dump_nodes(plan_nodes, f"{trace_dir}/010_plan.json")

    # Build each node and save state
    for i, node_name in enumerate(plan, start=2):
        node = env.get_node(node_name)
        if node.dirty or node.cache is None:
            env.build_node(node_name)
            state_file = f"{trace_dir}/{i:02}0_state.json"
            # Only dump nodes that are in the plan
            plan_nodes = [env.nodes[name] for name in plan]
            dump_nodes(plan_nodes, state_file)


def load_state_from_trace(
    env: Environment, trace_file: str, func_map: Dict[str, Callable[..., Any]]
) -> None:
    """Load environment state from a trace file.

    Args:
        env: Environment to load state into
        trace_file: Path to the trace file
        func_map: Mapping from node names to their functions
    """
    with open(trace_file) as f:
        content = f.read()
        decoder = json.JSONDecoder()
        pos = 0

        # Decode multiple JSON objects from the content
        while pos < len(content):
            # Skip whitespace
            while pos < len(content) and content[pos].isspace():
                pos += 1
            if pos >= len(content):
                break

            # Decode next object
            try:
                node_data, pos = decoder.raw_decode(content, pos)
                env.load_node_state(node_data, func_map)
            except json.JSONDecodeError as e:
                print(f"Error decoding JSON at position {pos}: {e}")
                raise
