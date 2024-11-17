from ailets.cons.typing import NodeDesc, Dependency

messages_to_query = NodeDesc(
    name="messages_to_query",
    inputs=[
        Dependency(source=".chat_messages"),
        Dependency(name="credentials", source=".credentials"),
        Dependency(name="toolspecs", source=".toolspecs"),
    ],
)

query = NodeDesc(
    name="query",
    inputs=[
        Dependency(name="messages", source="model.gpt4o.messages_to_query"),
    ],
    alias_of=".query",
)

response_to_markdown = NodeDesc(
    name="response_to_markdown",
    inputs=[
        Dependency(source="model.gpt4o.query"),
    ],
)

nodes = [messages_to_query, query, response_to_markdown]
