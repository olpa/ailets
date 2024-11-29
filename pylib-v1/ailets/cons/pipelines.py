from dataclasses import dataclass
from typing import Literal, Optional
import json
import tomllib
from typing import Sequence

from .typing import (
    Dependency,
    IEnvironment,
    INodeRegistry,
    IStream,
    Node,
)


@dataclass
class CmdlinePromptItem:
    value: str
    type: Literal["toml", "text", "file", "url"]
    content_type: Optional[str] = None
    toml: Optional[str] = None


def prompt_to_env(
    env: IEnvironment,
    prompt: Sequence[CmdlinePromptItem] = [CmdlinePromptItem("Hello!", "text")],
) -> None:
    def prompt_to_node(prompt_item: CmdlinePromptItem) -> None:
        if prompt_item.type == "toml":
            return

        def mk_node(prompt_content: str) -> Node:
            node = env.add_value_node(prompt_content.encode("utf-8"), explain="Prompt")
            env.alias(".prompt", node.name)
            return node

        if prompt_item.type == "text":
            content_item = {
                "type": "text",
                "text": prompt_item.value,
            }
            if prompt_item.toml:
                toml = tomllib.loads(prompt_item.toml)
                if toml.get("role", "").lower() == "system":
                    content_item["role"] = "system"
            mk_node(json.dumps(content_item))
            return

        assert prompt_item.content_type is not None, "Content type is required"
        base_content_type = prompt_item.content_type.split("/")[0]
        assert base_content_type in [
            "image"
        ], f"Unknown content type: {base_content_type}"
        assert prompt_item.type in [
            "url",
            "file",
        ], f"Unknown prompt item type: {prompt_item.type}"

        if prompt_item.type == "url":
            mk_node(
                json.dumps(
                    {
                        "type": base_content_type,
                        "url": prompt_item.value,
                        "content_type": prompt_item.content_type,
                    }
                )
            )
            return

        stream_name = env.get_next_name(f"media/{base_content_type}")
        node = mk_node(
            json.dumps(
                {
                    "type": base_content_type,
                    "stream": stream_name,
                    "content_type": prompt_item.content_type,
                }
            )
        )

        with open(prompt_item.value, "rb") as f:
            stream: IStream = env.create_new_stream(node.name, stream_name)
            stream.get_content().write(f.read())
            stream.close()

    for prompt_item in prompt:
        prompt_to_node(prompt_item)


def toml_to_env(
    env: IEnvironment,
    toml: Sequence[CmdlinePromptItem],
) -> None:
    for prompt_item in toml:
        if prompt_item.type != "toml":
            continue
        items = tomllib.loads(prompt_item.value)
        env.update_for_env_stream(items)


def toolspecs_to_env(
    env: IEnvironment, nodereg: INodeRegistry, tools: Sequence[str]
) -> None:
    for tool in tools:
        plugin_nodes = nodereg.get_plugin(f".tool.{tool}")
        schema = nodereg.get_node(plugin_nodes[0]).inputs[0].schema
        assert schema is not None, f"Tool {tool} has no schema"

        tool_spec = env.add_value_node(
            json.dumps(schema).encode("utf-8"),
            explain=f"Tool spec {tool}",
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
