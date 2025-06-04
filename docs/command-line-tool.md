# AI Command Line Tool "ailets"

A command-line interface for running AI models with various tools and state management capabilities.

## Usage

```bash
ailets MODEL [options]
```

## Required Arguments

- `MODEL`: The model to run. The best choices are `gpt`, `gemini`, or `claude`. To get the list of models, run the tool with a non-existing model name 'list'.

## Optional Arguments

- `--prompt TEXT`: Input prompt text (default: "Hello!"). Can be given multiple times.
- `--dry-run`: Perform a dry run without making changes
- `--save-state FILE`: Save execution state to specified file
- `--load-state FILE`: Load execution state from specified file
- `--one-step`: Execute only one step
- `--stop-before POINT`: Stop execution before specified point
- `--stop-after POINT`: Stop execution after specified point
- `--tool TOOL`: List of tools to use (e.g., get_user_name)
- `--opt KEY=VALUE`: Configuration options in `key=value` format. The value is parsed as JSON if possible, otherwise used as string. Most important keys are 'http.url' and 'llm.model'.
- `--download-to DIRECTORY`: Directory to download generated files to (default: "./out")
- `--file-system PATH` : Path to the virtual file system database in the Python `dbm.sqlite3` format
- `--debug`: Enable debug logging

## Examples

```base
# Run with a stdin prompt
echo "Hello!" | ailets gpt
# Output: Hello! How can I assist you today?
 
# Run with a direct prompt
ailets gpt --prompt "hello"
# Output: Hello! How can I assist you today?

# Use a local LLM on the port `8000`
ailets local --prompt "hello"

# Use an OpenAI-compatible endpoint
ailets gpt --prompt "hello" \
    --opt http.url=http://localhost:8000/v1/chat/completions --opt llm.model=custom-model

# Combined prompts
ailets gpt --prompt "What’s in this image?" --prompt @./image.jpeg
ailets gpt --prompt "What’s in this image?" --prompt "@https://upload.wikimedia.org/wikipedia/commons/thumb/d/dd/Gfp-wisconsin-madison-the-nature-boardwalk.jpg/2560px-Gfp-wisconsin-madison-the-nature-boardwalk.jpg"

# Combined prompt, with input from stdin
ailets gpt --prompt "Proofread the text, do not change the tone, target the B2 level:" --prompt @{text/plain}/dev/stdin

# Use a tool
ailets gpt --tool get_user_name --prompt "Hello!"
# Output: Hello, olpa! How can I assist you today?
# Note that my name is included in the output

# Dry run to see the dependency tree
ailets gpt --prompt "Hello!" --dry-run

# Stop at the specific point, save the state to a file
ailets gpt --prompt "Hello" --stop-before .query.17 --save-state state.json

# Load the state from a file
ailets gpt --load-state state.json --dry-run

# Execute one step at a time
ailets gpt --prompt "Hello" --one-step
```

## API key

Before using the tool, set an API key:

```bash
# For `gpt` models
export OPENAI_API_KEY=...

# For `gemini` models
export GOOGLEAPIS_API_KEY=...

# For `claude` models
export ANTHROPIC_API_KEY=...
```

## Input Formats

The `--prompt` argument can be specified multiple times and accepts several formats:

- `text`: Regular text prompt
  ```bash
  ailets gpt --prompt "Hello, how are you?"
  ```

- `@file`: Local file with auto-detected type
  ```bash
  ailets gpt --prompt "@image.jpg"
  ```

- `@{type}file`: Local file with explicit type
  ```bash
  ailets gpt --prompt "@{text/plain}input.txt"
  ```

- `@url`: URL with auto-detected type
  ```bash
  ailets gpt --prompt "@https://example.com/image.jpg"
  ```

- `@{type}url`: URL with explicit type
  ```bash
  ailets gpt --prompt "@{image/png}https://example.com/image.png"
  ```

- `-`: Read from stdin
  ```bash
  echo "Hello" | ailets gpt --prompt "-"
  ```

Supported content types:
- `text/*`: Text content
- `image/*`: Image content (both local files and URLs)

### Configuration Inside Prompt

Prompt can have a [TOML](https://toml.io/en/) configuration block. The tool will parse the block and make the values available to actors through a special stream called `env`.

There are two ways to write a TOML block:

One is to use a usual Markdown code block "toml":

```markdown
` ` ` toml
# ...
` ` `
```

Second way is to separate the TOML block from the prompt text with a line consisting of three dashes `---`.

### System Prompt

To provide a system prompt, add a TOML block with a `role="system"` item:

```bash
ailets gpt --prompt 'role="system"\n---\nYou are a helpful assistant who answers in Spanish' --prompt "Hello!"
# Output:
# ¡Hola! ¿En qué puedo ayudarte hoy?
```


## LLM vendor errors

`404 Not Found`: The tool accepts any model name as long as the base name is known. For example, the name `gpt-no-such-model` is valid for ailets because the base name is `gpt`, but since there is no such model, the vendor will return a 404 error.

`401 Unauthorized`: Bad API key.

`429 Too Many Requests`: The key has expired or all funds have been used.

Currently, there is no easy way to get detailed error information. One option is to observe what is sent to the vendor (`--stop-before .query.NN`) and then send the query manually using `wget` or `curl`.


## Model-specific Notes: gpt

For the list of the model-specific options, see the section "Create chat completion" at <https://platform.openai.com/docs/api-reference/chat>.

Below is an example of overriding `n`, `temperature`, disabling streaming, and using a system prompt:

```bash
ailets gpt --opt llm.n=3 --opt llm.temperature=0.8 --opt llm.stream=false --prompt '''role="system"
---
Generate answers with 3 sentences.
'''  --prompt "Hello!"
```

Output:

```
Hello! How can I assist you today? Feel free to ask me anything you're curious about.

Hello! How can I assist you today?

Hello! How can I assist you today? Feel free to ask me anything.
```

Without disabling streaming, the output will be mixed:

```
Hello!HelloHello How!! can How How I can can assist I I you assist assist today you you? today today I'm?? here I'm to here help to with help any with questions any or questions topics you you're have interested. in.
```


## Model-specific Notes: dall-e

Basic usage:

```bash
ailets dalle --prompt 'linux logo'
```

The output is a rewritten prompt and a link to the generated image.

```
Create an image of the Linux logo. It's a penguin known as Tux, standing upright, looking forward, and depicted using colors of black, white and yellow. The penguin is often shown in a simplistic and amusing style, with large white eyes, a bright yellow beak, and a white belly.

![image](https://oaidalleapiprodscus.blob.core.windows.net/private/org-....)
```

To get the image instead of the link, set the `response_format` parameter to `b64_json`:

```bash
ailets dalle --prompt $'response_format="b64_json"\n---\nlinux logo'
```


## Available Tools

Currently supported tools:

- `get_user_name`: Retrieves user name information


## Virtual File System

A Virtual File System (VFS) can be useful for:

- Passing multiple files to and from a dockerized tool
- Debugging communication between actors

The VFS is implemented as an SQLite3 database using Python's `dbm.sqlite3` format with:

- A single table named `Dict`
- Two columns: `key` and `value`, both of type `BLOB`

Common VFS operations:

```bash
# List all keys in the VFS
sqlite3 x.db "SELECT key FROM Dict;"

# Extract a file from the VFS
f=x.json
sqlite3 x.db "SELECT writefile('$f',value) FROM Dict WHERE key=CAST('$f' AS BLOB);"

# Insert a file into the VFS
f=tux.png
sqlite3 x.db "INSERT INTO Dict (key, value) VALUES (CAST('$f' AS BLOB), readfile('$f'));"
```

Example usage:

```bash
$ ailets gpt --prompt Hello --dry-run
├── .messages_to_markdown.18 [⋯ not built]
│   ├── .gpt.response_to_messages.17 [⋯ not built]
│   │   ├── .query.16 [⋯ not built]
│   │   │   ├── .gpt.messages_to_query.15 [⋯ not built]
│   │   │   │   ├── value.13 [✓ built] (chat messages)

$ rm -f x.db && touch x.db
$ ailets gpt --prompt Hello --file-system x.db
Hello! How can I assist you today?

$ sqlite3 x.db "SELECT key FROM Dict;"
.gpt.messages_to_query.15
.gpt.response_to_messages.17
.messages_to_markdown.18
.query.16
value.13

$ sqlite3 x.db "SELECT value FROM Dict WHERE key=CAST('value.13' AS BLOB);"
[{"type": "ctl"}, {"role": "user"}]
[{"type": "text"}, {"text": "Hello"}]

$ sqlite3 x.db "SELECT value FROM Dict WHERE key=CAST('.gpt.response_to_messages.17' AS BLOB);"
{"type":"ctl","role":"assistant"}
[{"type":"text"},{"text":"Hello! How can I assist you today?"}]
```

Together: Docker and VFS.

```
# Setup: Pass the database to the dockerized Ailets
OPENAI_API_KEY=sk-.....
ailets() {
  docker run --rm -e OPENAI_API_KEY=$OPENAI_API_KEY \
  --mount type=bind,src=$(pwd)/x.db,dst=/tmp/x.db --user `id -u` \
    olpa/ailets --file-system /tmp/x.db "$@"
}

# Re-create the database
rm -f x.db && touch x.db
ailets gpt --dry-run

# Put a file to the database and run Ailets
f=tux.png
sqlite3 x.db "INSERT INTO Dict (key, value) VALUES (CAST('$f' AS BLOB), readfile('$f'));"

ailets gpt --prompt "Describe the image." --prompt "@tux.png"
# The image features a cartoon penguin. It has ...
```
