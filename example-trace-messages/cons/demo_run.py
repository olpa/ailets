import os
import json
from cons import Environment


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

    # Initial state - dump just the plan
    os.makedirs(trace_dir, exist_ok=True)
    dump_nodes(plan_nodes, f"{trace_dir}/010_plan.json")

    # Build each node and save state
    for i, node_name in enumerate(plan, start=2):
        env.build_node(node_name)
        state_file = f"{trace_dir}/{i:02}0_state.json"
        # Only dump nodes that are in the plan
        plan_nodes = [env.nodes[name] for name in plan]
        dump_nodes(plan_nodes, state_file)
