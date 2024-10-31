from cons import mkenv, prompt_to_md
import os
import json

def dump_nodes(env, path: str) -> None:
    """Dump all nodes to a JSON file."""
    with open(path, "w") as f:
        for node in env.nodes.values():
            json.dump(node.to_json(), f, indent=2)
            f.write("\n")

def build_plan(env, target: str) -> None:
    """Build nodes in order, saving state after each build."""
    plan = env.plan(target)
    
    # Initial state
    os.makedirs("messages", exist_ok=True)
    dump_nodes(env, "messages/010_plan.json")
    
    # Build each node and save state
    for i, node_name in enumerate(plan, start=2):
        env.build_node(node_name)
        state_file = f"messages/{i:02}0_state.json"
        dump_nodes(env, state_file)

env = mkenv()
node = prompt_to_md(env)
build_plan(env, node.name)
