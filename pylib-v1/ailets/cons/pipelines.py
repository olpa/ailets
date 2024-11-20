import json
from typing import (
    Union,
    Tuple,
    Sequence,
)
from .typing import (
    Dependency,
    IEnvironment,
    INodeRegistry,
    NodeDesc,
    NodeDescFunc,
    Node,
)


def load_nodes_from_module(module: str, prefix: str) -> Sequence[NodeDescFunc]:
    try:
        imported_module = __import__(f"{prefix}.{module}", fromlist=["nodes"])
        if not hasattr(imported_module, "nodes"):
            raise AttributeError(f"Module {module} has no 'nodes' attribute")
        nodes = imported_module.nodes
        if not isinstance(nodes, list) or not all(
            isinstance(node, NodeDesc) for node in nodes
        ):
            raise TypeError(f"nodes from {module} must be a list of NodeDesc")
    except ImportError as e:
        raise ImportError(f"Could not import module {module}: {e}")

    return [
        NodeDescFunc(
            name=f"{module}.{node.name}" if module != "std" else node.name,
            inputs=node.inputs,
            func=getattr(
                __import__(f"{prefix}.{module}.{node.name}", fromlist=[node.name]),
                node.name,
            ),
        )
        for node in nodes
    ]


def must_get_tool_spec(env: IEnvironment, tool_name: str) -> Node:
    node_name = f"tool/{tool_name}/spec"
    (tool_spec_func, _) = env.get_tool(tool_name)
    return env.add_node(node_name, tool_spec_func)


def prompt_to_env(
    env: IEnvironment,
    prompt: Sequence[Union[str, Tuple[str, str]]] = ["Hello!"],
) -> None:
    def prompt_to_node(prompt_item: Union[str, Tuple[str, str]]) -> None:
        if isinstance(prompt_item, str):
            prompt_text = prompt_item
            prompt_type = "text"
        else:
            prompt_text, prompt_type = prompt_item
        node_tv = env.add_typed_value_node(prompt_text, prompt_type, explain="Prompt")
        env.alias(".prompt", node_tv.name)

    for prompt_item in prompt:
        prompt_to_node(prompt_item)


def toolspecs_to_env(
    env: IEnvironment, nodereg: INodeRegistry, tools: Sequence[str]
) -> None:
    for tool in tools:
        plugin_nodes = nodereg.get_plugin(f".tool.{tool}")
        schema = nodereg.get_node(plugin_nodes[0]).inputs[0].schema
        assert schema is not None, f"Tool {tool} has no schema"

        tool_spec = env.add_typed_value_node(
            json.dumps(schema), "json", explain=f"Tool spec {tool}"
        )
        env.alias(".toolspecs", tool_spec.name)
    else:
        env.alias(".toolspecs", None)


def instantiate_with_deps(
    env: IEnvironment,
    nodereg: INodeRegistry,
    target: str,
    aliases: dict[str, str],
) -> str:
    """Instantiate a node and its dependencies in the environment recursively.

    Args:
        env: Environment to add nodes to
        nodereg: Node registry containing node definitions
        target: Name of target node to instantiate, or a plugin name
        aliases: Map of node names to their aliases, takes precedence in resolution

    Returns:
        The created target node

    Raises:
        RuntimeError: If a dependency cycle is detected
    """
    # If target is a plugin name, get the last node from the plugin
    if not nodereg.has_node(target) and nodereg.has_plugin(target):
        target = nodereg.get_plugin(target)[-1]

    resolve = aliases.copy()  # Start with provided aliases
    created_nodes = set()  # Track which nodes we need to set up dependencies for
    visiting: set[str] = set()  # Track nodes being visited for cycle detection

    def create_node_recursive(node_name: str, parent_node_name) -> None:
        node_name = resolve.get(node_name, node_name)

        # Skip if node already exists in environment
        if env.has_node(node_name):
            return

        # Check for cycles
        if node_name in visiting:
            cycle = " -> ".join(list(visiting) + [node_name])
            raise RuntimeError(f"Dependency cycle detected: {cycle}")

        visiting.add(node_name)

        # Create dependencies first
        try:
            node_desc = nodereg.get_node(node_name)
        except KeyError:
            parent_context = f" (required by '{parent_node_name}')"
            raise RuntimeError(
                f"Node '{node_name}' not found in registry while building "
                f"pipeline{parent_context}.\n"
            )
        for dep in node_desc.inputs:
            create_node_recursive(dep.source, node_name)

        # Create the node
        node = env.add_node(name=node_name, func=node_desc.func)
        resolve[node_name] = node.name
        created_nodes.add(node_name)

        visiting.remove(node_name)

    create_node_recursive(target, ".")

    # Second pass: set up all dependencies
    for node_name in created_nodes:
        try:
            node_desc = nodereg.get_node(node_name)
        except KeyError:
            raise RuntimeError(
                f"Node '{node_name}' not found in registry while building pipeline."
            )
        deps = []
        for dep in node_desc.inputs:
            # Try to resolve dependency name through the resolve mapping
            source = resolve.get(dep.source, dep.source)
            if source != dep.source:
                source = resolve.get(source, source)
            deps.append(
                Dependency(
                    name=dep.name, source=source, stream=dep.stream, schema=dep.schema
                )
            )
        env.depend(resolve[node_name], deps)

    return resolve[target]
