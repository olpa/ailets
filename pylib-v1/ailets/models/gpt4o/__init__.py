from ailets.cons.typing import NodeDesc, Dependency

messages_to_query = NodeDesc(
    name="messages_to_query",
    inputs=[
        Dependency(source=".initial_chat_messages"),
        Dependency(source=".added_chat_messages"),
        Dependency(name="credentials", source=".credentials"),
        Dependency(name="toolspecs", source=".toolspecs"),
    ],
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

nodes = [messages_to_query, query, response_to_markdown]
