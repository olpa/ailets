import base64
import json
from typing import Awaitable, Callable, TextIO, Tuple, Optional, Set
import json
from typing import Dict, Any

from ailets.cons.async_buf import AsyncBuffer
from ailets.cons.atyping import Dependency, INodeRuntime, Node
from ailets.cons.seqno import Seqno
from ailets.cons.streams import Stream
from ailets.cons.util import to_basename
from ailets.cons.environment import Environment

from .plugin import NodeRegistry


def dependency_to_json(
    dep: Dependency,
) -> tuple[Optional[str], str, Optional[str], Optional[dict[str, Any]]]:
    return dep.astuple()

def load_dependency(
    obj: tuple[Optional[str], str, Optional[str], Optional[dict[str, Any]]],
) -> Dependency:
    return Dependency.from_tuple(*obj)


def dump_node(node: Node, f: TextIO) -> None:
    json.dump({
        "name": node.name,
        "deps": [dependency_to_json(dep) for dep in node.deps],
        "explain": node.explain,  # Add explain field to JSON
        # Skip func as it's not serializable
    }, f, indent=2)

def load_node(
        node_json: Dict[str, Any],
        nodereg: NodeRegistry,
        seqno: Seqno,
) -> Node:
    name = node_json["name"]

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

    if "." in name:
        loaded_suffix = int(name.split(".")[-1])
        seqno.at_least(loaded_suffix + 1)
    
    deps = [load_dependency(dep) for dep in node_json["deps"]]

    node = Node(
        name=name,
        func=func,
        deps=deps,
        explain=node_json.get("explain"),
    )
    return node


async def dump_stream(stream: Stream, f: TextIO) -> None:
    b = await stream.read(pos=0, size=-1)
    try:
        content_field = "content"
        content = b.decode("utf-8")
    except UnicodeDecodeError:
        content_field = "b64_content"
        content = base64.b64encode(b).decode("utf-8")
    json.dump({
        "node": stream.node_name,
        "name": stream.stream_name,
        "is_closed": stream.is_closed(),
        content_field: content,
    }, f, indent=2)

async def load_stream(data: dict[str, Any]) -> Stream:
    if "b64_content" in data:
        content = base64.b64decode(data["b64_content"])
    else:
        content = data["content"].encode("utf-8")
    buf = AsyncBuffer()
    await buf.write(content)
    if data["is_closed"]:
        await buf.close()
    return Stream(
        node_name=data["node"],
        stream_name=data["name"],
        buf=buf,
    )

async def dump_environment(env: Environment, f: TextIO) -> None:
    for node in env.dagops.nodes.values():
        dump_node(node, f)
        f.write("\n")
    for alias, names in env.dagops.aliases.items():
        json.dump({"alias": alias, "names": list(names)}, f, indent=2)
        f.write("\n")
    for stream in env.streams._streams.values():
        await dump_stream(stream, f)
        f.write("\n")
    json.dump({"env": env.for_env_stream}, f, indent=2)
    f.write("\n")


async def load_environment(f: TextIO, nodereg: NodeRegistry) -> Environment:
    env = Environment()
    
    content = f.read()
    decoder = json.JSONDecoder()
    pos = 0

    # Decode multiple JSON objects from the content
    while pos < len(content):
        while pos < len(content) and content[pos].isspace():
            pos += 1
        if pos >= len(content):
            break

        # Decode next object
        try:
            obj_data, pos = decoder.raw_decode(content, pos)
            if "deps" in obj_data:
                node = load_node(obj_data, nodereg, env.seqno)
                env.dagops.nodes[node.name] = node
            elif "is_closed" in obj_data:
                stream = await load_stream(obj_data)
                env.streams.add_stream(stream)
            elif "alias" in obj_data:
                env.dagops.aliases[obj_data["alias"]] = obj_data["names"]
            elif "env" in obj_data:
                env.for_env_stream.update(obj_data["env"])
            else:
                raise ValueError(f"Unknown object data: {obj_data}")
        except json.JSONDecodeError as e:
            print(f"Error decoding JSON at position {pos}: {e}")
            raise

    return env

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
