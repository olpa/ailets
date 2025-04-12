import base64
import dataclasses
import json
from typing import (
    Any,
    Awaitable,
    Callable,
    Dict,
    List,
    Optional,
    Set,
    TextIO,
    Tuple,
)

from ailets.cons.atyping import (
    Dependency,
    Errors,
    IKVBuffers,
    INodeRegistry,
    INodeRuntime,
    IPipe,
    IPiper,
    IProcesses,
    Node,
)
from ailets.cons.dagops import Dagops
from ailets.cons.seqno import Seqno
from ailets.cons.util import to_basename
from ailets.cons.environment import Environment


def dependency_to_json(
    dep: Dependency,
) -> dict[str, Any]:
    return dataclasses.asdict(dep)


def load_dependency(
    obj: dict[str, Any],
) -> Dependency:
    return Dependency(**obj)


def dump_node(node: Node, is_finished: bool, f: TextIO) -> None:
    json.dump(
        {
            "name": node.name,
            "deps": [dependency_to_json(dep) for dep in node.deps],
            "explain": node.explain,
            "is_finished": is_finished,
            # Skip func as it's not serializable
        },
        f,
        indent=2,
    )


def load_node(
    node_json: Dict[str, Any],
    nodereg: INodeRegistry,
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
        node_desc = nodereg.get_node(base_name)
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


async def dump_pipe(path: str, pipe: IPipe, f: TextIO) -> None:
    writer = pipe.get_writer()
    json.dump(
        {
            "pipe": path,
            "is_closed": writer.closed,
        },
        f,
        indent=2,
    )


async def load_pipe(piper: IPiper, data: dict[str, Any]) -> None:
    path = data["pipe"]
    is_closed = data.get("is_closed", False)
    pipe = piper.create_pipe(path, "", open_mode="append")
    if is_closed:
        writer = pipe.get_writer()
        writer.close()


async def dump_kv_item(kv: IKVBuffers, path: str, f: TextIO) -> None:
    b = kv.open(path, "read").borrow_mut_buffer()
    try:
        content_field = "content"
        content = b.decode("utf-8")
    except UnicodeDecodeError:
        content_field = "b64_content"
        content = base64.b64encode(b).decode("utf-8")
    json.dump(
        {
            "path": path,
            content_field: content,
        },
        f,
        indent=2,
    )


async def load_kv_item(kv: IKVBuffers, data: dict[str, Any]) -> None:
    if "b64_content" in data:
        content = base64.b64decode(data["b64_content"])
    else:
        content = data["content"].encode("utf-8")
    path = data["path"]
    item = kv.open(path, "write")
    item.borrow_mut_buffer()[:] = content


def save_env_values(env: Environment, f: TextIO) -> None:
    json.dump(
        {
            "env": env.for_env_pipe,
            "errno": env.errno.value,
        },
        f,
        indent=2,
    )


def load_env_values(env: Environment, data: dict[str, Any]) -> None:
    env.for_env_pipe.update(data["env"])
    env.errno = Errors(data["errno"])


async def dump_environment(env: Environment, f: TextIO) -> None:
    for node in env.dagops.nodes.values():
        dump_node(node, is_finished=env.processes.is_node_finished(node.name), f=f)
        f.write("\n")
    for alias, names in env.dagops.aliases.items():
        json.dump({"alias": alias, "names": list(names)}, f, indent=2)
        f.write("\n")
    for path in env.kv.listdir(""):
        await dump_kv_item(env.kv, path, f)
        f.write("\n")
    for path, pipe in env.piper.pipes.items():
        await dump_pipe(path, pipe, f)
        f.write("\n")
    save_env_values(env, f)
    f.write("\n")


async def load_environment(f: TextIO, nodereg: INodeRegistry) -> Environment:
    env = Environment(nodereg)

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
                if obj_data.get("is_finished", False):
                    env.processes.add_finished_node(node.name)
            elif "path" in obj_data:
                await load_kv_item(env.kv, obj_data)
            elif "pipe" in obj_data:
                await load_pipe(env.piper, obj_data)
            elif "alias" in obj_data:
                env.dagops.aliases[obj_data["alias"]] = obj_data["names"]
            elif "env" in obj_data:
                load_env_values(env, obj_data)
            else:
                raise ValueError(f"Unknown object data: {obj_data}")
        except json.JSONDecodeError as e:
            print(f"Error decoding JSON at position {pos}: {e}")
            raise

    return env


def print_dependency_tree(
    dagops: Dagops,
    processes: IProcesses,
    node_name: str,
    indent: str = "",
    visited: Optional[Set[str]] = None,
    slot_name: str = "",
) -> None:
    """Print a tree showing node dependencies and build status.

    Args:
        node_name: Name of the node to print
        indent: Current indentation string (used for recursion)
        visited: Set of visited nodes to prevent cycles
        slot_name: Optional slot name to display
    """
    if visited is None:
        visited = set()

    node = dagops.get_node(node_name)
    if node_name.startswith("defunc."):
        status = "\033[90mdefunc\033[0m"
    else:
        status = (
            "\033[32m✓ built\033[0m"
            if processes.is_node_finished(node_name)
            else (
                "\033[35m⚡ active\033[0m"
                if processes.is_node_active(node_name)
                else "\033[33m⋯ not built\033[0m"
            )
        )

    # Print current node with explanation if it exists
    display_name = node.name
    if slot_name:
        display_name = f"{display_name}.{slot_name}"

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
    deps_by_param: Dict[str, List[Tuple[str, str]]] = {}
    for dep in dagops.iter_deps(node_name):
        if dep.name not in deps_by_param:
            deps_by_param[dep.name] = []
        deps_by_param[dep.name].append((dep.source, dep.slot))

    next_indent = f"{indent}│   "

    # Print default dependencies (param_name is None)
    for dep_name, slot_name in deps_by_param.get("", []):
        print_dependency_tree(
            dagops, processes, dep_name, next_indent, visited.copy(), slot_name
        )

    # Print named dependencies grouped by parameter
    for param_name, dep_names in deps_by_param.items():
        if param_name:  # Skip "" group as it's already printed
            print(f"{next_indent}├── (param: {param_name})")
            param_indent = f"{next_indent}│   "
            for dep_name, slot_name in dep_names:
                print_dependency_tree(
                    dagops,
                    processes,
                    dep_name,
                    param_indent,
                    visited.copy(),
                    slot_name,
                )

    visited.remove(node_name)
