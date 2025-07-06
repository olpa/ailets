from ailets.atyping import NodeDesc, Dependency


toolcall_to_messages = NodeDesc(
    name="toolcall_to_messages",
    inputs=[
        Dependency(source=".tool_output"),
        Dependency(name="llm_tool_spec", source=".llm_tool_spec"),
    ],
)

query = NodeDesc(
    name="query",
    inputs=[
        Dependency(source="to-be-overridden"),
    ],
)

messages_to_markdown = NodeDesc(
    name="messages_to_markdown",
    inputs=[
        Dependency(source=".output_messages"),
    ],
)

nodes = [
    toolcall_to_messages,
    query,
    messages_to_markdown,
]
