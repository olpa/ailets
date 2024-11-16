from ailets.cons.typing import NodeDesc, Dependency

messages_to_query = NodeDesc(
    name="messages_to_query",
    inputs=[
        Dependency(source="prompt_to_messages"),
        Dependency(name="credentials", source="credentials"),
        Dependency(name="toolspecs", source="toolspecs"),
    ],
)

response_to_markdown = NodeDesc(
    name="response_to_markdown",
    inputs=[
        Dependency(source="query"),
    ],
)

nodes = [messages_to_query, response_to_markdown]
