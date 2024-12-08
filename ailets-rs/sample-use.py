from ailets_rs import MessagesToMarkdown

converter = MessagesToMarkdown()

messages = [{
    "role": "user",
    "content": [{"type": "text", "text": "Hello"}]
}]

markdown = converter.convert(messages)
print(markdown)
