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

def serialize_node(self, name: str, stream: TextIO) -> None:
    """Serialize a node's state to a JSON stream."""
    if name not in self.nodes:
        raise KeyError(f"Node {name} not found")

    json.dump(self.nodes[name].to_json(), stream, indent=2)
    stream.write("\n")

def print_dependency_tree(
    self,
    node_name: str,
    indent: str = "",
    visited: Optional[Set[str]] = None,
    stream_name: Optional[str] = None,
) -> None:
    """Print a tree showing node dependencies and build status.

    Args:
        node_name: Name of the node to print
        indent: Current indentation string (used for recursion)
        visited: Set of visited nodes to prevent cycles
        stream_name: Optional stream name to display
    """
    if visited is None:
        visited = set()

    node = self.get_node(node_name)
    if node_name.startswith("defunc."):
        status = "\033[90mdefunc\033[0m"
    else:
        status = (
            "\033[32m✓ built\033[0m"
            if self.is_node_built(node_name)
            else "\033[33m⋯ not built\033[0m"
        )

    # Print current node with explanation if it exists
    display_name = node.name
    if stream_name is not None:
        display_name = f"{display_name}.{stream_name}"

    node_text = f"{indent}├── {display_name} [{status}]"
    if node.explain:
        node_text += f" ({node.explain})"
    print(node_text)

    # Track visited nodes to prevent cycles
    if node_name in visited:
        print(f"{indent}│   └── (cycle detected)")
        return
    visited.add(node_name)

    # Group dependencies by parameter name
    deps_by_param: Dict[Optional[str], List[Tuple[str, Optional[str]]]] = {}
    for dep in self.iter_deps(node_name):
        if dep.name not in deps_by_param:
            deps_by_param[dep.name] = []
        deps_by_param[dep.name].append((dep.source, dep.stream))

    next_indent = f"{indent}│   "

    # Print default dependencies (param_name is None)
    for dep_name, stream_name in deps_by_param.get(None, []):
        self.print_dependency_tree(
            dep_name, next_indent, visited.copy(), stream_name
        )

    # Print named dependencies grouped by parameter
    for param_name, dep_names in deps_by_param.items():
        if param_name is not None:  # Skip None group as it's already printed
            print(f"{next_indent}├── (param: {param_name})")
            param_indent = f"{next_indent}│   "
            for dep_name, stream_name in dep_names:
                self.print_dependency_tree(
                    dep_name, param_indent, visited.copy(), stream_name
                )

    visited.remove(node_name)

def load_node_state(self, node_data: Dict[str, Any], nodereg: NodeRegistry) -> Node:
    """Load a node's state from JSON data.

    Args:
        node_data: Node state from JSON

    Returns:
        The loaded node
    """
    name = node_data["name"]

    # Try to get function from map, if not found and name has a number suffix,
    # try without the suffix
    func: Callable[[INodeRuntime], Awaitable[None]]
    base_name = to_basename(name)
    if base_name.startswith("defunc."):
        base_name = base_name[7:]
    if base_name == "value":
        # Special case for typed value nodes
        async def func(
            _: INodeRuntime,
        ) -> None: ...

    else:
        node_desc = nodereg.nodes.get(base_name)
        if node_desc is None:
            raise KeyError(f"No function registered for node: {name} ({base_name})")
        func = node_desc.func

    # Update counter if needed to stay above loaded node's suffix
    if "." in name:
        loaded_suffix = int(name.split(".")[-1])
        if self._seqno <= loaded_suffix:
            self._seqno = loaded_suffix + 1

    # Create new node with loaded state
    node = Node(
        name=name,
        func=func,
        deps=[Dependency.from_json(dep) for dep in node_data["deps"]],
        explain=node_data.get("explain"),  # Load explain field if present
    )
    self.nodes[name] = node
    return node