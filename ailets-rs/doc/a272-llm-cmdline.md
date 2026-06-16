# Developer Task: LLM Command-Line Arguments for `dagsh`

## Goal

Extend the `dagsh` CLI (`ailets-rs/cli`) so it can be invoked in a non-interactive, LLM-style manner. Each call builds a prompt from command-line arguments, creates the corresponding DAG nodes, then runs `dagsh` as usual (interactive shell or `-l` script). If stdin is consumed by a prompt argument, the interactive session is skipped.

## Target UX

```bash
# Text prompts — each arg is its own node
dagsh "Summarize this document"
dagsh "Context:" @notes.txt "What does this mean?"

# File inputs (@-prefix reads file)
dagsh @diagram.png "Explain the architecture"

# Stdin (all three forms are equivalent)
cat README.md | dagsh "Summarize this"   # TTY check
dagsh - "Summarize this"                 # explicit -
dagsh @- "Summarize this"               # explicit @-

# System prompt
dagsh --system-prompt "You are a helpful assistant" "Hello"

# Combined with a load script (no interactive session)
cat README.md | dagsh --system-prompt "Answer in French" @notes.txt "Compare" -l run.tcl
```

## Node Structure

For each invocation, the CLI creates value nodes and aliases them all as `input`. The `messages_to_query` actor accumulates all `input` aliases in order.

The full node sequence created (full `ContentItem` JSON in each `value` node):

```
value [{"type":"ctl"},{"role":"system"}]          → alias input  (only if --system-prompt)
value [{"type":"text"},{"text":"<system text>"}]  → alias input  (only if --system-prompt)
value [{"type":"ctl"},{"role":"user"}]             → alias input  (always, before first positional)
value <item 1>                                     → alias input
value <item 2>                                     → alias input
...
value <stdin item>                                 → alias input  (appended last, if stdin used)
```

## Argument Parsing

### Positional arguments

- Plain string → `ContentItemText`: `[{"type":"text"},{"text":"..."}]`
- `@path` or `@{attrs}path` → read file, produce `ContentItem` based on detected or specified type:
  - Text extensions (`.txt`, `.md`, `.rs`, `.py`, …) → `ContentItemText`
  - Image extensions (`.png`, `.jpg`, `.gif`, `.webp`, …) → `ContentItemImage` with `image_key` (stored in KV)
- `-` or `@-` → use the stdin node (see below)

### `@{...}` attribute override syntax

The `{...}` block overrides keys in `[0]` (the attrs dict of the `ContentItem`):

- `@{image/png}file.bin` — contains `/` but no `=`: shorthand for `content_type=image/png`
- `@{content_type=image/png,detail=high}file.bin` — explicit `key=value` pairs, comma-separated

> **Note:** The exact `@{...}` syntax design is still under discussion. Implement auto-detection from file extension as the baseline. The `@{...}` block is a separate design decision; leave a clearly marked extension point in the parser.

### `--system-prompt TEXT`

Inserts a ctl(system) node and a text node before the ctl(user) node. May be given only once (simplest case for now).

### Stdin

Three triggers:
1. Explicit `-` positional arg — stdin node inserted **at that position**
2. Explicit `@-` positional arg — stdin node inserted **at that position**
3. Implicit: stdin is not a TTY and at least one positional arg is present — stdin node appended **after all positional args**

In all cases, use the existing stdin node (not a `value` node).

If stdin is consumed (any of the three triggers), do **not** start an interactive session. Still run the `-l` script if one was provided.

## Session Behaviour

| Positional args present? | Stdin consumed? | `-l` script? | Result |
|---|---|---|---|
| No | No | No | Interactive shell (existing behaviour) |
| No | No | Yes | Load script, then interactive shell (existing behaviour) |
| Yes | No | No | Create nodes + aliases, start interactive shell |
| Yes | No | Yes | Create nodes + aliases, run script, exit |
| Yes | Yes | No | Create nodes + aliases, exit |
| Yes | Yes | Yes | Create nodes + aliases, run script, exit |

## Implementation

### `src/shell_ui.rs`

Extend `CliArgs`:

```rust
pub struct CliArgs {
    pub load_script: Option<String>,       // existing
    pub prompt_items: Vec<PromptArg>,      // new
    pub system_prompt: Option<String>,     // new
}

pub enum PromptArg {
    Text(String),
    File { path: String, attrs: Vec<(String, String)> },
    Stdin,
}
```

Extend `parse_args`:
- `--system-prompt TEXT`: consume next arg as value
- `-` → `PromptArg::Stdin`
- `@-` → `PromptArg::Stdin`
- `@path` or `@{...}path` → `PromptArg::File`
- Any other non-flag arg → `PromptArg::Text`

Update `print_usage` to document the new flags.

### `src/main.rs`

After `parse_args`, if `prompt_items` is non-empty or `system_prompt` is set:

1. Check for implicit stdin (TTY check); if triggered, append `PromptArg::Stdin`
2. Build the sequence of `value` TCL commands and execute them via `shell.execute`
3. Check whether stdin was consumed; if so, suppress the interactive loop

Leave a clearly marked extension point where the `@{...}` attr-override block will be parsed.

### File reading

```rust
fn file_to_content_item(path: &str, attrs: &[(String, String)]) -> Result<String, String>
```

- Detect content type from extension (override with `attrs` if provided)
- Text: read UTF-8, return `[{"type":"text"},{"text":"..."}]`
- Image: store bytes in KV under a generated key, return `[{"type":"image","content_type":"..."},{"image_key":"..."}]`
- Unknown extension without explicit attrs: return a clear error

## Files to Change

| File | Change |
|---|---|
| `src/shell_ui.rs` | `CliArgs`, `PromptArg`, `parse_args`, `print_usage` |
| `src/main.rs` | node creation, session dispatch |
| `Cargo.toml` | add `is-terminal` if not already present |

## Acceptance Criteria

```bash
# 1. text prompt creates input nodes, interactive shell opens
dagsh "Summarize this" <<< "help"  # shell opens and responds to "help"

# 2. file prompt reads file
echo "42 is the answer" > /tmp/t.txt
dagsh @/tmp/t.txt "What is the answer?"  # no interactive session, nodes created

# 3. explicit stdin consumed, no interactive session
echo "the sky is blue" | dagsh - "What color is the sky?"

# 4. implicit stdin consumed (TTY check), no interactive session
echo "the sky is blue" | dagsh "What color is the sky?"

# 5. system prompt node created before user nodes
dagsh --system-prompt "Answer only in French" "Hello"

# 6. combined with -l script: script runs, no interactive session
echo "context" | dagsh "Process this" -l run.tcl

# 7. no positional args: existing interactive shell unchanged
echo "help" | dagsh
```

---

# Research notes (scratch)

### claude code

$ claude "say hello"

Wrong, only one positional argument is expected:
$ claude say hello

$ claude -p "query"
$ claude --print "query"
+ stdin in this case
Looks like "-p" is put before stdin

Claude: somehow handles file paths in "-p" prompt

Second positional argument: path to an image


--system-prompt
--system-prompt-file
--append-system-prompt
--append-system-prompt-file

# codex

$ codex "prompt as one argument"

non-interactive:

$ codex exec "some prompt"
$ codex e "some prompt"

Reads stdin, appends to the prompt

$ cat content.md | codex exec "summarize this document"

Using "-" as prompt forces stdin

"--image" or "-i"

- can be comma-separated
- can be repeated

System prompt override: in config or in the command line argument "-c"

### pi coding agent

```
$ pi [options] [@files...] [messages...]
```

First positional argument becomes the initial message
pre-submitted

```bash
pi "Read package.json" "What dependencies do we have?"
```

-p:
TTY check

```bash
cat README.md | pi -p "Summarize this text"
git diff | pi -p "Review these changes"
echo "context" | pi -p "Process this"
```

```bash
pi @prompt.md "Answer this"
pi -p @screenshot.png "What's in this image?"
pi @code.ts @test.ts "Review these files"
pi @context.md @diagram.png "Explain the architecture"
```

"-p"

### My plan

- Each positional argument: own item in the prompt
- Prefixed with @: read from file. Text items are supported
- --system prompt, --append-system-prompt. Several items in the prompt
