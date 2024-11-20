from ailets.cons.typing import NodeDesc, Dependency

prompt_to_messages = NodeDesc(
    name="prompt_to_messages",
    inputs=[
        Dependency(source=".prompt"),
        Dependency(name="type", source=".prompt", stream="type"),
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

stdout = NodeDesc(
    name="stdout",
    inputs=[
        Dependency(source=".model_output"),
    ],
)

nodes = [prompt_to_messages, toolcall_to_messages, query, stdout]
