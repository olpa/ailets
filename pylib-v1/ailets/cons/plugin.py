from typing import Protocol, Sequence

from .typing import NodeDesc, NodeDescFunc


class IRegistry(Protocol):
    def add_node_def(self, node_def: NodeDescFunc) -> None: ...

    def add_plugin_nodes(self, plugin_nodes: Sequence[NodeDescFunc]) -> None: ...


def load_plugin(registry: IRegistry, pypackage: str, regname: str) -> None:
    """Load a plugin module and register its nodes.

    Args:
        registry: Registry to add nodes to
        module: Name of the module to load (e.g. "std", "llm")
        prefix: Base package path for node modules

    Raises:
        ImportError: If module cannot be imported
        AttributeError: If module has no 'nodes' attribute
        TypeError: If nodes are not a list of NodeDesc
    """
    try:
        # Import the module containing node definitions
        imported_module = __import__(f"{pypackage}", fromlist=["nodes"])
        if not hasattr(imported_module, "nodes"):
            raise AttributeError(f"Module {pypackage} has no 'nodes' attribute")

        nodes = imported_module.nodes
        if not isinstance(nodes, list) or not all(
            isinstance(node, NodeDesc) for node in nodes
        ):
            raise TypeError(f"nodes from {pypackage} must be a list of NodeDesc")

        # Convert each NodeDesc to NodeDescFunc and register
        for node in nodes:
            # Import the actual node function
            node_module = __import__(f"{pypackage}.{node.name}", fromlist=[node.name])
            node_func = getattr(node_module, node.name)

            # Create NodeDescFunc
            node_desc = NodeDescFunc(
                name=f"{regname}.{node.name}", inputs=node.inputs, func=node_func
            )

            # Register the node
            registry.add_node_def(node_desc)

    except ImportError as e:
        raise ImportError(f"Could not load plugin {regname}: {e}")
