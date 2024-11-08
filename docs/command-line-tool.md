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

## Available Tools

Currently supported tools:
- `get_user_name`: Retrieves user name information

## Examples

```bash
export OPENAI_API_KEY=sk-......

# Run with stdin prompt
echo "Hello!" | ailets gpt4o
# Output: Hello! How can I assist you today?

# Run with direct prompt
ailets gpt4o --prompt "hello"
# Output: Hello! How can I assist you today?

# Use specific tools
./ailets gpt4o --tool get_user_name --prompt "Hello!"
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
```