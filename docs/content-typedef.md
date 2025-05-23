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

Text content with the following fields:

- `type: "text"`
- `text: str` - The actual text content

See also [OpenAI reference](https://platform.openai.com/docs/guides/text).


### `ContentItemImage`

Image content with the following fields:

- `type: "image"|"input_image"|"output_image"`
- `content_type: Optional<str>` – MIME type of the image, for example `image/png`
- `detail: Optional<"low"|"high"|"auto" | str>` – Level of detail to use when processing and understanding the image
- Either of, but not both:
  - `image_url: str`, or
  - `image_key: str`

See also [OpenAI reference](https://platform.openai.com/docs/guides/images-vision).

There are three ways to include an image:

- Image reference using a normal URL.
- Image blob using a data URL in the form `data:[<mediatype>][;base64],<base64-encoded data>`.
- Image blob using a key in a key-value storage.

#### Image reference

For image references, ailets do not download the data. Instead, the logic is:

- Pass the URL to an LLM model as is.
- Insert the URL into markdown when serializing.

It is expected that the provider will fetch the image. OpenAI GPT and Anthropic Claude do so, while Gemini limits fetching only to images uploaded to Gemini.

For OpenAI-compatible chats, the value of `content_type` is ignored. Vendors do type auto-detection for image references but not for image blobs.

#### Image blob

For image blobs, passing them to a model depends on the specific model. For OpenAI-compatible chats, data URLs are passed as they are.

When serializing an image blob to markdown:

- For a data URL, decode the blob and store it in the key-value storage.
- When using a key, copy the blob to a new key with the prefix `out/`. In markdown, reference the `out/` version. Expect that the ailets runner will extract `out/*` blobs for the user.


### `ContentItemFunction`

Function call with fields:

- `type: "function"`
- `id: str` - Function call identifier
- `function: dict` containing:
    - `name: str` - Function name
    - `arguments: str` - Function arguments

There is no content item representing the result of a function call. Instead, the `ChatMessage` with the role `tool` is used for that purpose. The tool result is then represented as `Content`.