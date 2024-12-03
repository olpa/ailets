from ailets.cons.atyping import NodeDesc, Dependency

messages_to_query = NodeDesc(
    name="messages_to_query",
    inputs=[
        Dependency(source=".chat_messages"),
    ],
)

query = NodeDesc(
    name="query",
    inputs=[
        Dependency(source="messages_to_query"),
    ],
    alias_of=".query",
)

response_to_messages = NodeDesc(
    name="response_to_messages",
    inputs=[
        Dependency(source="query"),
    ],
)

nodes = [messages_to_query, query, response_to_messages]
