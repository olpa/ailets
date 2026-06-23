# dagsh command-line reference

## Synopsis

```
dagsh [OPTIONS] [PROMPT_ITEMS...]
```

Without prompt items, dagsh opens an interactive shell. With prompt items, it
builds the corresponding DAG nodes before entering the shell (or exits if stdin
was consumed — see [Session behaviour](#session-behaviour)).

## Options

| Flag | Description |
|------|-------------|
| `-l <file>`, `--load <file>` | Run a TCL script on startup. May be given more than once; scripts run in order. |
| `--system-prompt <text>` | Add a system-prompt item at this position in the prompt sequence. |
| `--model <alias\|id>` | LLM model: a short alias (see below) or a full model ID. |
| `--llm-url <url>` | LLM endpoint URL. Overrides the URL derived from a model alias. |
| `--llm-thinking <level>` | Reasoning effort: `off`, `low`, `medium`, `high`. |
| `-h`, `--help` | Print usage and exit. |

## Prompt items

Prompt items are positional arguments that build the LLM prompt. They are
processed left to right and each item produces one or more DAG nodes.

### Plain text

Any argument that does not start with `-` or `@` is a user-role text item.

```bash
dagsh "Summarize this document"
dagsh "Context:" @notes.txt "What does this mean?"
```

### File (`@path`)

`@path` reads the file at `path`. The content type is detected from the file
extension:

- **Text** (`.txt`, `.md`, `.rs`, `.py`, `.js`, `.ts`, `.json`, `.toml`,
  `.yaml`, `.yml`, `.html`, `.css`, `.sh`) — passed as a text content item.
- **Image** (`.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`) — stored in the KV
  store and passed as an image content item with the appropriate MIME type.

```bash
dagsh @diagram.png "Explain the architecture"
dagsh @notes.txt "Summarise this"
```

### File with attribute overrides (`@key=value,...,file=path`)

When the string after `@` contains `=`, it is parsed as a comma-separated list
of `key=value` pairs. The `file=` key is required and provides the path; all
other pairs override extension-based detection.

```bash
dagsh @type=text,file=script.tcl "Review this"
dagsh @type=image,content_type=image/png,file=photo.dat "Describe"
```

### Stdin (`-` or `@-`)

Both `-` and `@-` wire stdin into the DAG as a `file_value` actor node at that
position in the prompt sequence. When stdin is consumed this way, dagsh exits
after running any load scripts rather than opening the interactive shell.

```bash
cat README.md | dagsh - "Summarise this"
cat README.md | dagsh @- "Summarise this"
```

### Model aliases

`--model` accepts a short alias or a full model ID. Known aliases:

| Alias | Model ID |
|-------|----------|
| `gpt` | `gpt-5.4` |
| `gpt-mini` | `gpt-5.4-mini` |
| `fable` | `claude-fable-5` |
| `opus` | `claude-opus-4-8` |
| `sonnet` | `claude-sonnet-4-6` |
| `haiku` | `claude-haiku-4-5` |
| `gemini` | `gemini-2.5-flash` |
| `flash` | `gemini-3.5-flash` |
| `local` | _(user-defined, routes to local Ollama)_ |

A short alias sets both the model ID and the provider URL. Passing `--llm-url` explicitly overrides the URL derived from an alias. A full model ID is passed verbatim; set the URL separately if needed.

### Environment variables

| Variable | Overridden by |
|----------|--------------|
| `AILETS_MODEL` | `--model` |
| `AILETS_LLM_URL` | `--llm-url` (or URL implied by `--model` alias) |
| `AILETS_LLM_THINKING` | `--llm-thinking` |
| `AILETS_LLM_STREAM` | _(no CLI flag)_ |

## System prompt

`--system-prompt <text>` inserts a system-role item at that position. It may
appear anywhere in the argument list and interleaves naturally with user items.

```bash
dagsh --system-prompt "You are a helpful assistant" "Hello"
dagsh --system-prompt "Answer in French" @notes.txt "Compare"
```

## Examples

```bash
# Text prompt — opens interactive shell with nodes already created
dagsh "Summarize this document"

# File input
dagsh @diagram.png "Explain the architecture"

# Explicit stdin — exits after DAG nodes are created (no interactive shell)
cat README.md | dagsh - "Summarize this"

# System prompt with file input
dagsh --system-prompt "Answer only in French" @notes.txt "What is the main point?"

# Force text type on an extension-less file
dagsh @type=text,file=config "Explain these settings"

# Load a script after building the prompt nodes, then exit
cat data.txt | dagsh - "Process this" -l pipeline.tcl

# Multiple load scripts
dagsh -l setup.tcl -l run.tcl

# Model with thinking level
dagsh --model opus --llm-thinking high @report.md "Summarize"
```
