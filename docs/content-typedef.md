# Content type definition

To have interoperability between different models, we define a common content type, based on [OpenAI chat completion messages](https://platform.openai.com/docs/api-reference/chat/create).

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

### `ContentItemText`

Text content with fields:

- `type: "text"`
- `text: str` - The actual text content

See also [OpenAI reference](https://platform.openai.com/docs/guides/text).


### `ContentItemImage`

Image content with fields:

- `type: "image"` 
- `content_type: Optional<str>` - MIME type of the image, for example `image/png`
- Either of but not both:
  - `url: str`, or
  - `key: str`

For `url`, some models produce and accept data URLs. Ailets should prefer `stream` over `url`, where `stream` is a named file stream inside ailets' runtime.

The recommended stream name is `media/image.*`.

If the stream name is `out/*`, ailets will save the file to the output directory. The name of the file is the md5 of the stream content.


### `ContentItemFunction`

Function call with fields:

- `type: "function"`
- `id: str` - Function call identifier
- `function: dict` containing:
    - `name: str` - Function name
    - `arguments: str` - Function arguments

There is no content item representing the result of a function call. Instead, the `ChatMessage` with the role `tool` is used for that purpose. The tool result is then represented as `Content`.