import json
from typing import Callable, Union, Tuple, Sequence
from .typing import IEnvironment, NodeDesc, NodeDescFunc, INodeRuntime, Node


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


def get_func_map(
    nodes: Sequence[NodeDescFunc],
) -> dict[str, Callable[[INodeRuntime], None]]:
    """Create mapping of node names to their functions."""
    return {
        "typed_value": lambda _: None,
        **{node.name: node.func for node in nodes},
    }


def must_get_tool_spec(env: IEnvironment, tool_name: str) -> Node:
    node_name = f"tool/{tool_name}/spec"
    (tool_spec_func, _) = env.get_tool(tool_name)
    return env.add_node(node_name, tool_spec_func)


def nodelib_to_env(env: IEnvironment, nodelib: Sequence[NodeDescFunc]) -> None:
    func_map = get_func_map(nodelib)
    for node_desc in nodelib:
        node_func = func_map[node_desc.name]
        node = env.add_node(node_desc.name, node_func, node_desc.inputs)
        env.alias(node_desc.name, node.name)


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


def alias_basenames(env: IEnvironment, nodes: Sequence[NodeDescFunc]) -> None:
    for node in nodes:
        if "." in node.name:
            resolved_name = env.get_node(node.name).name
            env.alias(node.name.split(".")[-1], resolved_name)


def toolspecs_to_env(
    env: IEnvironment, nodeset: Sequence[NodeDescFunc], tools: Sequence[str]
) -> None:
    for tool in tools:
        begin_node_name = f"{tool}.call"  # TODO FIXME
        begin_node_desc = next(
            (node for node in nodeset if node.name == begin_node_name), None
        )
        assert begin_node_desc is not None, f"Tool {tool} has no begin node"

        schema = begin_node_desc.inputs[0].schema
        assert schema is not None, f"Tool {tool} has no schema"

        tool_spec = env.add_typed_value_node(
            json.dumps(schema), "json", explain=f"Tool {tool}"
        )
        env.alias("toolspecs", tool_spec.name)
    else:
        env.alias("toolspecs", None)
