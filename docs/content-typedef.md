# Content type definition

To have interoperability between different models, we define a common content type.

Each model is expected to get a sequence of `ChatMessage`s as input and produce a sequence of `ChatMessage`s as output.

## `ChatMessage`

A `ChatMessage` represents a message in a chat conversation. It is defined as a TypedDict with two required fields:

- `role`: A literal string that must be one of: "system", "user", "assistant", or "tool"
  - `system`: The system prompt
  - `user`: A user message
  - `assistant`: An assistant message
  - `tool`: Tool output
- `content`: A sequence of content items

## `ContentItem`

`ContentItemText`: Text content with fields:

- `type: "text"`
- `text: str` - The actual text content

`ContentItemRefusal`: Refusal message with fields:

- `type: "refusal"`
- `refusal: str` - The refusal message

`ContentItemImage`: Image content with fields:

- `type: "image"` 
- `content_type: str` - MIME type of the image, for example `image/png`
- Either `url: str` or `stream: str` (but not both)

For `url`, some models produce and accept data URLs. Ailets should prefer `stream` over `url`, where `stream` is a named file stream inside ailets' runtime.

`ContentItemFunction`: Function call with fields:

- `type: "function"`
- `id: str` - Function call identifier
- `function: dict` containing:
    - `name: str` - Function name
    - `arguments: str` - Function arguments

There is no content item representing the result of a function call. Instead, the `ChatMessage` with the role `tool` is used for that purpose. The tool result is then represented as `Content`.