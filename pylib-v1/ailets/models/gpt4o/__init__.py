from ailets.cons.typing import NodeDesc, Dependency

messages_to_query = NodeDesc(
    name="messages_to_query",
    inputs=[
        Dependency(source="chat_messages"),
        Dependency(name="credentials", source="credentials"),
        Dependency(name="toolspecs", source="toolspecs"),
    ],
)

query = NodeDesc(
    name="query",
    inputs=[
        Dependency(source="messages_to_query"),
    ],
    alias_of="std.query",
)

response_to_markdown = NodeDesc(
    name="response_to_markdown",
    inputs=[
        Dependency(source="query"),
    ],
)

nodes = [messages_to_query, query, response_to_markdown]
