from ailets.cons.typing import NodeDesc, Dependency

prompt_to_messages = NodeDesc(
    name="prompt_to_messages",
    inputs=[
        Dependency(source="prompt"),
        Dependency(name="type", source="prompt", stream="type"),
    ],
)

credentials = NodeDesc(
    name="credentials",
    inputs=[],
)

query = NodeDesc(
    name="query",
    inputs=[
        Dependency(source="messages_to_query"),
    ],
)

stdout = NodeDesc(
    name="stdout",
    inputs=[
        Dependency(source="response_to_markdown"),
    ],
)

nodes = [prompt_to_messages, credentials, query, stdout]