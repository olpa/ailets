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


def build_plan_writing_trace(
    env: Environment,
    target: str,
    trace_dir: str,
    one_step: bool = False,
    initial_counter: int = 1,
) -> None:
    """Build nodes in order, saving state after each build.

    Args:
        env: Environment to build in
        target: Target node to build
        trace_dir: Directory to write trace files to
        one_step: If True, build only one step and exit
        initial_counter: Starting value for state counter
    """
    os.makedirs(trace_dir, exist_ok=True)
    state_counter = initial_counter

    # Get initial plan
    plan = env.plan(target)
    plan_nodes = [env.nodes[name] for name in plan]
    current_node_count = len(env.nodes)

    # Dump initial plan if starting fresh
    if state_counter == 1:
        dump_nodes(plan_nodes, f"{trace_dir}/010_plan.json")
        state_counter += 1

    while True:
        # Find next dirty node to build
        next_node = None
        for node_name in plan:
            node = env.get_node(node_name)
            if node.dirty or node.cache is None:
                next_node = node
                break

        # If no dirty nodes, we're done
        if next_node is None:
            break

        # Build the node
        env.build_node(next_node.name)

        # Check if number of nodes changed
        new_node_count = len(env.nodes)
        if new_node_count != current_node_count:
            # Recalculate plan
            plan = env.plan(target)
            current_node_count = new_node_count

        # Save state after build
        state_file = f"{trace_dir}/{state_counter:02}0_state.json"
        plan_nodes = [env.nodes[name] for name in plan]
        dump_nodes(plan_nodes, state_file)
        state_counter += 1

        if one_step:  # Exit after building one node if requested
            break


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
