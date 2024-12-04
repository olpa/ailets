import json
from typing import TextIO
import json
from typing import Dict, Any

from .cons import Environment
from .plugin import NodeRegistry

async def dump_environment(env: Environment, f: TextIO) -> None:
    """Convert environment to JSON file.
    
    Args:
        env: Environment to serialize
        f: Text file to write to
    """
    # Save nodes
    for node in env.nodes.values():
        json.dump(node.to_json(), f, indent=2)
        f.write("\n")

    # Save streams
    await env._streams.to_json(f)

    # Save environment stream data
    json.dump({"env": env._for_env_stream}, f, indent=2)
    f.write("\n")

    # Save aliases
    for alias, names in env._aliases.items():
        json.dump({"alias": alias, "names": list(names)}, f, indent=2)
        f.write("\n")

async def load_environment(f: TextIO, nodereg: NodeRegistry) -> Environment:
    """Create environment from JSON file.
    
    Args:
        f: Text file to read from
        nodereg: Node registry for function lookup
        
    Returns:
        Loaded Environment instance
    """
    env = Environment()
    
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
            obj_data, pos = decoder.raw_decode(content, pos)
            if "deps" in obj_data:
                env.load_node_state(obj_data, nodereg)
            elif "is_closed" in obj_data:
                await env._streams.add_stream_from_json(obj_data)
            elif "alias" in obj_data:
                env._aliases[obj_data["alias"]] = obj_data["names"]
            elif "env" in obj_data:
                env._for_env_stream.update(obj_data["env"])
            else:
                raise ValueError(f"Unknown object data: {obj_data}")
        except json.JSONDecodeError as e:
            print(f"Error decoding JSON at position {pos}: {e}")
            raise

    return env
