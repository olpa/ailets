# AI Command Line Tool "ailets"

A command-line interface for running AI models with various tools and state management capabilities.

## Usage

```bash
ailets MODEL [options]
```

## Required Arguments

- `MODEL`: The model to run (currently only supports 'gpt4o')

## Optional Arguments

- `--prompt TEXT`: Input prompt text (default: "-" for stdin)
- `--dry-run`: Perform a dry run without making changes
- `--save-state FILE`: Save execution state to specified file
- `--load-state FILE`: Load execution state from specified file
- `--one-step`: Execute only one step
- `--stop-at POINT`: Stop execution at specified point
- `--tool TOOL [TOOL ...]`: List of tools to use (e.g., get_user_name)

## API key

Before using the tool, make sure to set your OpenAI API key:

```bash
export OPENAI_API_KEY=sk-your-api-key-here
```

## Available Tools

Currently supported tools:
- `get_user_name`: Retrieves user name information

## Input Formats

The `--prompt` argument can be specified multiple times and accepts several formats:

- `text`: Regular text prompt
  ```bash
  ailets gpt4o --prompt "Hello, how are you?"
  ```

- `@file`: Local file with auto-detected type
  ```bash
  ailets gpt4o --prompt "@image.jpg"
  ```

- `@{type}file`: Local file with explicit type
  ```bash
  ailets gpt4o --prompt "@{text}input.txt"
  ```

- `@url`: URL with auto-detected type
  ```bash
  ailets gpt4o --prompt "@https://example.com/image.jpg"
  ```

- `@{type}url`: URL with explicit type
  ```bash
  ailets gpt4o --prompt "@{image_url}https://example.com/image.jpg"
  ```

- `-`: Read from stdin
  ```bash
  echo "Hello" | ailets gpt4o --prompt "-"
  ```

Supported content types:
- `text`: Text content
- `image_url`: Image content (both local files and URLs)

## Examples

```base
# Run with stdin prompt
echo "Hello!" | ailets gpt4o
# Output: Hello! How can I assist you today?
 
# Run with direct prompt
ailets gpt4o --prompt "hello"
# Output: Hello! How can I assist you today?

# Use a tool
ailets gpt4o --tool get_user_name --prompt "Hello!"
# Output: Hello, olpa! How can I assist you today?
# Note that my name is included in the output

# Dry run to see dependency tree
ailets gpt4o --prompt "Hello!" --dry-run

# Save state to file
ailets gpt4o --prompt "Hello!" --save-state state.json

# Load state from file
ailets gpt4o --load-state state.json --dry-run

# Execute one step at a time
ailets gpt4o --prompt "Hello" --one-step

# Stop at specific point
ailets gpt4o --prompt "Hello" --stop-at messages_to_query.5

# Multiple prompts
ailets gpt4o --prompt "What’s in this image?" --prompt @./image.jpeg
ailets gpt4o  --prompt "What’s in this image?" --prompt "@https://upload.wikimedia.org/wikipedia/commons/thumb/d/dd/Gfp-wisconsin-madison-the-nature-boardwalk.jpg/2560px-Gfp-wisconsin-madison-the-nature-boardwalk.jpg"
```
