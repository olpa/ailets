from typing import Dict, Sequence

from .atyping import Dependency, INodeRegistry, NodeDesc, NodeDescFunc


class NodeRegistry(INodeRegistry):
    def __init__(self) -> None:
        self.nodes: Dict[str, NodeDescFunc] = {}
        self.plugin_node_names: Dict[str, Sequence[str]] = {}

    def get_node(self, name: str) -> NodeDescFunc:
        return self.nodes[name]

    def has_node(self, name: str) -> bool:
        return name in self.nodes

    def has_plugin(self, regname: str) -> bool:
        return regname in self.plugin_node_names

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
                (e.g. ".tool.get_user_name")

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

            # First pass to resolve names
            resolve = {}
            for node in nodes:
                if node.alias_of:
                    resolve[node.name] = node.alias_of
                else:
                    resolve[node.name] = f"{regname}.{node.name}"

            # Convert each NodeDesc to NodeDescFunc and register
            plugin_node_names = []
            for node in nodes:

                node_func = node
                if node_func and node_func.alias_of:
                    node_func = self.nodes[node_func.alias_of]
                    func = node_func.func
                else:
                    node_module = __import__(
                        f"{pypackage}.{node.name}", fromlist=[node.name]
                    )
                    func = getattr(node_module, node.name)

                # Create NodeDescFunc
                node_desc = NodeDescFunc(
                    name=resolve[node.name],
                    inputs=[
                        Dependency(
                            name=dep.name,
                            source=resolve.get(dep.source, dep.source),
                            stream=dep.stream,
                            schema=dep.schema,
                        )
                        for dep in node.inputs
                    ],
                    func=func,
                )

                # Register the node
                self.add_node_def(node_desc)
                plugin_node_names.append(resolve[node.name])

            self.add_plugin(regname, plugin_node_names)

        except ImportError as e:
            raise ImportError(f"Could not load plugin {regname}: {e}")


def hijack_msg2md(nodereg: NodeRegistry) -> None:
    from ailets.stdlib.messages_to_markdown_wasm import (
        messages_to_markdown_wasm,
        load_wasm_module,
    )

    load_wasm_module()

    orig_msg2md = nodereg.get_node(".messages_to_markdown")

    new_msg2md = NodeDescFunc(
        name=".messages_to_markdown",
        inputs=orig_msg2md.inputs,
        func=messages_to_markdown_wasm,
    )

    nodereg.add_node_def(new_msg2md)
