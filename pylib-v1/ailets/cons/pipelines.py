from .typing import NodeDesc, NodeDescFunc, INodeRuntime
from .cons import Environment, Node
from typing import Callable, Union, Tuple, Sequence


def load_nodes_from_module(module: str) -> Sequence[NodeDescFunc]:
    try:
        imported_module = __import__(f"ailets.cons.nodes.{module}", fromlist=["nodes"])
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
            name=node.name,
            inputs=node.inputs,
            func=getattr(
                __import__(
                    f"ailets.cons.nodes.{module}.{node.name}", fromlist=[node.name]
                ),
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


def must_get_tool_spec(env: Environment, tool_name: str) -> Node:
    node_name = f"tool/{tool_name}/spec"
    (tool_spec_func, _) = env.get_tool(tool_name)
    return env.add_node(node_name, tool_spec_func)


def system_to_env(env: Environment, system: str) -> Sequence[NodeDescFunc]:
    nodes = load_nodes_from_module(system)
    func_map = get_func_map(nodes)
    for node_desc in nodes:
        node_func = func_map[node_desc.name]
        node = env.add_node(node_desc.name, node_func, node_desc.inputs)
        env.alias(node_desc.name, node.name)
    return nodes


def prompt_to_md(
    env: Environment,
    system: str,
    prompt: Sequence[Union[str, Tuple[str, str]]] = ["Hello!"],
) -> Sequence[NodeDescFunc]:
    nodes_std = system_to_env(env, "std")
    nodes_sys = system_to_env(env, system)

    # Create nodes for each prompt item
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

    # TODO: Add tool spec nodes

    # TODO: validate that all the deps are valid
    return [*nodes_std, *nodes_sys]
