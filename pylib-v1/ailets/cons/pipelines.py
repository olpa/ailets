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
        env.alias("prompt", node_tv.name)

    for prompt_item in prompt:
        prompt_to_node(prompt_item)


def toolspecs_to_env(
    env: IEnvironment, nodereg: INodeRegistry, tools: Sequence[str]
) -> None:
    for tool in tools:
        plugin_nodes = nodereg.get_plugin(f"tool.{tool}")
        schema = plugin_nodes[0].inputs[0].schema
        assert schema is not None, f"Tool {tool} has no schema"

        tool_spec = env.add_typed_value_node(
            json.dumps(schema), "json", explain=f"Tool spec {tool}"
        )
        env.alias("toolspecs", tool_spec.name)
    else:
        env.alias("toolspecs", None)


def instantiate_plugin(
    env: IEnvironment,
    nodereg: INodeRegistry,
    name: str,
) -> Sequence[Node]:
    """Instantiate a plugin's nodes in the environment.

    Args:
        env: Environment to add nodes to
        nodereg: Node registry containing plugin definitions
        name: Name of plugin to instantiate
    """
    created_nodes = []
    resolve = {}

    # First create all nodes
    for node_name in nodereg.get_plugin(name):
        node_desc = nodereg.nodes[node_name]

        node = env.add_node(
            name=node_name, func=node_desc.func, explain=f"Plugin node {node_name}"
        )

        created_nodes.append(node)
        resolve[node_name] = node.name

    # Then update dependencies after all nodes exist
    for node_name in nodereg.get_plugin(name):
        node_full_name = resolve[node_name]
        node_desc = nodereg.nodes[node_name]
        deps = []
        for dep in node_desc.inputs:
            # Try to resolve dependency name through the resolve mapping
            source = resolve.get(dep.source, dep.source)
            deps.append(
                Dependency(
                    name=dep.name, source=source, stream=dep.stream, schema=dep.schema
                )
            )
        env.depend(node_full_name, deps)

    return created_nodes
