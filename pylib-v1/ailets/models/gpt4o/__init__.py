from ailets.cons.typing import NodeDesc, Dependency

messages_to_query = NodeDesc(
    name="messages_to_query",
    inputs=[
        Dependency(source=".chat_messages"),
        Dependency(name="credentials", source="credentials"),
        Dependency(name="toolspecs", source=".toolspecs"),
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
    alias_of=".query",
)

response_to_markdown = NodeDesc(
    name="response_to_markdown",
    inputs=[
        Dependency(source="query"),
    ],
)

nodes = [messages_to_query, credentials, query, response_to_markdown]
