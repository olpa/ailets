from nodes import prompt_to_md
from cons import mkenv
import os
import json

env = mkenv()
result = prompt_to_md(env)

# Create messages directory if it doesn't exist
os.makedirs("messages", exist_ok=True)

# Get the plan and serialize nodes
plan = env.plan(result.name)
with open("messages/10_plan.json", "w") as f:
    for node_name in plan:
        json.dump(env.nodes[node_name].to_json(), f, indent=2)
        f.write("\n")
