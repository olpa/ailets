from ailets.cons.atyping import NodeDesc, Dependency

prompt_to_messages = NodeDesc(
    name="prompt_to_messages",
    inputs=[
        Dependency(source=".prompt"),
    ],
)

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
        Dependency(source=".model_output"),
    ],
)

tmptmp = NodeDesc(
    name="tmptmp",
    inputs=[],
)

stdout = NodeDesc(
    name="stdout",
    inputs=[
        Dependency(source=".messages_to_markdown"),
        #Dependency(source=".tmptmp"),
    ],
)

nodes = [
    prompt_to_messages,
    toolcall_to_messages,
    query,
    messages_to_markdown,
    tmptmp,
    stdout,
]
