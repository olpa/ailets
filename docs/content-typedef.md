# Content type definition

To have interoperability between different models, we define a common content type.

Each model is expected to get a sequence of `ContentItem`s as input and produce a sequence of `ContentItem`s as output.


## `ContentItem`

Logically, a content item is a dictionary. Physically, it's a tuple of two dictionaries.

- The first dictionary contains attributes. This dictionary is finite and small, with a size of less than 256 bytes.
- The second dictionary contains the content. It holds one item, which can be potentially very large.

The reason for having a tuple is to enforce the order of attributes, allowing streaming transformations. In particular, we want to see the attribute `type` before we get the actual content.


## `ContentItemText`

Text content with the following fields:

- `[0].type: "text"`
- `[1].text: str` - The actual text content

See also [OpenAI reference](https://platform.openai.com/docs/guides/text).


## `ContentItemImage`

Image content with the following fields:

- `[0].type: "image"|"input_image"|"output_image"`
- `[0].content_type: Optional<str>` – MIME type of the image, for example `image/png`
- `[0].detail: Optional<"low"|"high"|"auto" | str>` – Level of detail to use when processing and understanding the image
- Either of, but not both:
  - `[1].image_url: str`, or
  - `[1].image_key: str`

See also [OpenAI reference](https://platform.openai.com/docs/guides/images-vision).

There are three ways to include an image:

- Image reference using a normal URL.
- Image blob using a data URL in the form `data:[<mediatype>][;base64],<base64-encoded data>`.
- Image blob using a key in a key-value storage.

### Image reference

For image references, ailets do not download the data. Instead, the logic is:

- Pass the URL to an LLM model as is.
- Insert the URL into markdown when serializing.

It is expected that the provider will fetch the image. OpenAI GPT and Anthropic Claude do so, while Gemini limits fetching only to images uploaded to Gemini.

For OpenAI-compatible chats, the value of `content_type` is ignored. Vendors do type auto-detection for image references but not for image blobs.

### Image blob

For image blobs, passing them to a model depends on the specific model. For OpenAI-compatible chats, data URLs are passed as they are.

When serializing an image blob to markdown:

- For a data URL, decode the blob and store it in the key-value storage.
- When using a key, copy the blob to a new key with the prefix `out/`. In markdown, reference the `out/` version. Expect that the ailets runner will extract `out/*` blobs for the user.


## `ContentItemCtl`

Affect the processing of the following items. Currently, its main use is to annotate "who" said the message. Additionally, if we decide to implement OpenAI choices (multiple outputs), the control message could indicate which choice comes next.

- `[0].type: "ctl"`
- `[1].role: "system"|"user"|"assistant"|"tool"|str`

See also [OpenAI chat completion messages](https://platform.openai.com/docs/api-reference/chat/create).


## `ContentItemToolSpecs`

Create a `tools` section to inform the LLM about what it can use. See the OpenAI documentation on [function calling](https://platform.openai.com/docs/guides/function-calling) for details.

- `[0].type: "toolspecs"`
- Either of, but not both:
  - `[1].toolspecs: FunctionSpec[]`, or
  - `[1].toolspecs_key: str`

For `toolspecs`, the value is an array of function specifications in the OpenAI format.

For `toolspecs_key`, the function specifications are read from the given key. The format is extended JSONL: instead of being wrapped in an array, the objects are immediately on the top level and _not_ divided by commas.


## `ContentItemFunction`

An artifact of the serialization-deserialization process when calling functions for an LLM. See the OpenAI documentation on [function calling](https://platform.openai.com/docs/guides/function-calling) for more details.

- `[0].type: "function"`
- `[0].id: str` - Function call identifier
- `[0].name: str` - Function name
- `[1].arguments: str` - Function arguments

As a user, you should not create `function` items, but instead use `toolspec` items.

There is no content item representing the result of a function call. Instead, the `ChatMessage` with the role `tool` is used for that purpose. The tool result is then represented as `content` items.

You should not mix function-items with content-items such as text, image etc in one message.
