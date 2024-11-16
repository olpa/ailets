from typing import Dict, Sequence

from .typing import INodeRegistry, NodeDesc, NodeDescFunc


class NodeRegistry(INodeRegistry):
    def __init__(self) -> None:
        self.nodes: Dict[str, NodeDescFunc] = {}
        self.plugin_node_names: Dict[str, Sequence[str]] = {}

    def add_node_def(self, node_def: NodeDescFunc) -> None:
        self.nodes[node_def.name] = node_def

    def add_plugin(self, regname: str, plugin_node_names: Sequence[str]) -> None:
        self.plugin_node_names[regname] = plugin_node_names

    def get_plugin(self, regname: str) -> Sequence[str]:
        return self.plugin_node_names[regname]

    def load_plugin(self, pypackage: str, regname: str) -> None:
        """Load a plugin module and register its nodes.

        Args:
            pypackage: Python package path to load plugin from
                (e.g. "ailets.tools.get_user_name")
            regname: Registration prefix for the plugin's nodes
                (e.g. "tool.get_user_name")

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
                node_module = __import__(
                    f"{pypackage}.{node.name}", fromlist=[node.name]
                )
                node_func = getattr(node_module, node.name)

                # Create NodeDescFunc
                node_desc = NodeDescFunc(
                    name=f"{regname}.{node.name}", inputs=node.inputs, func=node_func
                )

                # Register the node
                self.add_node_def(node_desc)

            self.add_plugin(regname, [f"{regname}.{node.name}" for node in nodes])

        except ImportError as e:
            raise ImportError(f"Could not load plugin {regname}: {e}")
