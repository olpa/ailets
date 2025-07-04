from dataclasses import dataclass
import dataclasses
from io import StringIO
from typing import Any, Literal, Mapping, Optional, Set
import json
from typing import Sequence
import sys
import os.path
import logging

from ailets.cons.util import open_file

if sys.version_info >= (3, 11):
    import tomllib
else:
    import tomli as tomllib

from ailets.atyping import (
    ContentItemCtl,
    ContentItemImage,
    ContentItemText,
    ContentItemToolSpec,
    Dependency,
    IDagops,
    IEnvironment,
    INodeRegistry,
)


logger = logging.getLogger(__name__)


@dataclass
class CmdlinePromptItem:
    value: str
    type: Literal["toml", "text", "file", "url", "tool"]
    content_type: Optional[str] = None
    toml: Optional[str] = None
    node_name: Optional[str] = None


@dataclass
class AnnotatedPromptItem:
    role: str
    prompt_item: CmdlinePromptItem


def annotate_prompt(
    prompt: Sequence[CmdlinePromptItem],
) -> Sequence[AnnotatedPromptItem]:
    result = []
    for item in prompt:
        if item.type == "toml":
            continue

        role = "user"
        if item.toml:
            try:
                toml_data = tomllib.loads(item.toml)
                if toml_data.get("role", "").lower() == "system":
                    role = "system"
            except Exception:
                pass

        result.append(AnnotatedPromptItem(role=role, prompt_item=item))
    return result


async def prompt_to_dagops(
    env: IEnvironment,
    prompt: Sequence[CmdlinePromptItem] = [CmdlinePromptItem("Hello!", "text")],
) -> None:
    chat_messages = StringIO()

    def role_to_messages(role: str) -> None:
        ctl_item: ContentItemCtl = (
            {
                "type": "ctl",
            },
            {
                "role": role,
            },
        )
        json.dump(ctl_item, chat_messages)
        chat_messages.write("\n")

    async def prompt_to_node(prompt_item: CmdlinePromptItem) -> None:
        if prompt_item.type == "toml":
            return

        if prompt_item.type == "text":
            text_item: ContentItemText = (
                {
                    "type": "text",
                },
                {
                    "text": prompt_item.value,
                },
            )
            json.dump(text_item, chat_messages)
            chat_messages.write("\n")
            return

        if prompt_item.type == "tool":
            assert prompt_item.value is not None, "Tool name is required for tool type"
            assert prompt_item.node_name is not None, "Tool node name is required"
            toolspec_item: ContentItemToolSpec = (
                {
                    "type": "toolspec",
                },
                {
                    "toolspec_key": prompt_item.node_name,
                },
            )
            json.dump(toolspec_item, chat_messages)
            chat_messages.write("\n")
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
            url_image_item: ContentItemImage = (
                {
                    "type": "image",
                    "content_type": prompt_item.content_type,
                },
                {
                    "image_url": prompt_item.value,
                },
            )
            json.dump(url_image_item, chat_messages)
            chat_messages.write("\n")
            return  # return if "url"

        assert prompt_item.node_name is not None, "Image node name is required"
        key_image_item: ContentItemImage = (
            {
                "type": "image",
                "content_type": prompt_item.content_type,
            },
            {
                "image_key": prompt_item.node_name,
            },
        )
        json.dump(key_image_item, chat_messages)
        chat_messages.write("\n")

    annotated_prompt = annotate_prompt(prompt)
    last_role = None
    for prompt_item in annotated_prompt:
        role: Optional[str] = prompt_item.role
        if prompt_item.prompt_item.type == "tool":
            role = None
            last_role = None
        if role != last_role and role is not None:
            role_to_messages(role)
            last_role = role
        await prompt_to_node(prompt_item.prompt_item)

    node = env.dagops.add_value_node(
        chat_messages.getvalue().encode("utf-8"),
        env.piper,
        env.processes,
        explain="chat messages",
    )
    env.dagops.alias(".chat_messages", node.name)


def toml_to_env(
    env: IEnvironment,
    opts_kv: Mapping[str, Any],
    opts_ls: Sequence[tuple[str, Any]],
    toml: Sequence[CmdlinePromptItem],
) -> None:
    for key, value in opts_kv.items():
        env.for_env_pipe[key] = value
    for key, value in opts_ls:
        env.for_env_pipe[key] = value
    for prompt_item in toml:
        if prompt_item.type != "toml":
            continue
        items = tomllib.loads(prompt_item.value)
        env.for_env_pipe.update(items)


def media_to_dagops(
    env: IEnvironment, whole_prompt: Sequence[CmdlinePromptItem]
) -> Sequence[CmdlinePromptItem]:
    new_prompt = []
    for prompt_item in whole_prompt:
        if prompt_item.type != "file":
            new_prompt.append(prompt_item)
            continue
        b = os.path.basename(prompt_item.value)

        with open_file(env.kv, prompt_item.value) as f:
            bytes = f.read()
        media_spec = env.dagops.add_value_node(
            bytes,
            env.piper,
            env.processes,
            explain=f"file {b}",
        )

        new_item = dataclasses.replace(prompt_item, node_name=media_spec.name)
        new_prompt.append(new_item)

    return new_prompt


def media_to_alias(env: IEnvironment, whole_prompt: Sequence[CmdlinePromptItem]) -> str:
    media = [prompt_item for prompt_item in whole_prompt if prompt_item.type == "file"]
    alias = env.dagops.get_next_name("media")
    for media_item in media:
        env.dagops.alias(alias, media_item.node_name)
    else:
        env.dagops.alias(alias, None)
    return alias


def toolspecs_to_dagops(
    env: IEnvironment, tools: Sequence[str]
) -> Sequence[CmdlinePromptItem]:
    prompt = []

    for tool in tools:
        plugin_nodes = env.nodereg.get_plugin(f".tool.{tool}")
        schema = env.nodereg.get_node(plugin_nodes[0]).inputs[0].schema
        assert schema is not None, f"Tool {tool} has no schema"

        tool_spec = env.dagops.add_value_node(
            json.dumps(schema).encode("utf-8"),
            env.piper,
            env.processes,
            explain=f"toolspec {tool}",
        )

        prompt.append(
            CmdlinePromptItem(type="tool", value=tool, node_name=tool_spec.name)
        )

    return prompt


def toolspecs_to_alias(env: IEnvironment, tools: Sequence[CmdlinePromptItem]) -> str:
    alias = env.dagops.get_next_name("toolspecs")
    for tool in tools:
        env.dagops.alias(alias, tool.node_name)
    else:
        env.dagops.alias(alias, None)
    return alias


def instantiate_with_deps(
    dagops: IDagops,
    nodereg: INodeRegistry,
    target: str,
    aliases: dict[str, str],
) -> str:
    """Instantiate a node and its dependencies in the environment recursively.

    Args:
        dagops: Dagops to add nodes to
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

    def create_node_recursive(node_name: str, parent_node_name: str) -> None:
        node_name = resolve.get(node_name, node_name)

        # Skip if node already exists in environment
        if dagops.has_node(node_name):
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
                f"flow{parent_context}.\n"
            )
        for dep in node_desc.inputs:
            create_node_recursive(dep.source, node_name)

        # Create the node
        node = dagops.add_node(name=node_name, func=node_desc.func)
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
                f"Node '{node_name}' not found in registry while building a flow"
            )
        deps = []
        for dep in node_desc.inputs:
            # Try to resolve dependency name through the resolve mapping
            source = resolve.get(dep.source, dep.source)
            if source != dep.source:
                source = resolve.get(source, source)
            deps.append(
                Dependency(
                    name=dep.name, source=source, slot=dep.slot, schema=dep.schema
                )
            )
        dagops.depend(resolve[node_name], deps)

    return resolve[target]


def dup_output_to_stdout(env: IEnvironment, node_names: Set[str]) -> None:
    for node_name in node_names:
        node_env = env.for_env_pipe.setdefault(node_name, {})
        if not isinstance(node_env, dict):
            logger.warning(f"Env setup for node {node_name} is not a dict")
            continue
        node_handles = node_env.setdefault("handles", {})
        if not isinstance(node_handles, dict):
            logger.warning(f"Handles for env setup of node {node_name} is not a dict")
            continue
        node_handles["1"] = "print"
