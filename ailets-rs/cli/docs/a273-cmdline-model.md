# A273: LLM Model and Endpoint Selection on the Command Line

## Goal

Allow users to select the LLM model, provider endpoint, and thinking level
without editing TCL scripts or DAG definitions — through CLI flags and through
environment variables, in that priority order.

---

## Proposed interface

### CLI flags

```
dagsh --model <model-id> [...]
dagsh --llm-url <endpoint-url> [...]
dagsh --llm-thinking <level> [...]
```

`--model` accepts a short alias or a full model ID:

| Form | Example | Meaning |
|------|---------|---------|
| Short alias | `sonnet`, `opus`, `haiku` | Well-known shorthand resolved to a full model ID |
| Full model ID | `claude-sonnet-4-6` | Passed verbatim to the provider |

`--llm-url` accepts the full endpoint URL:

```
--llm-url https://api.anthropic.com/v1/messages
--llm-url http://localhost:11434/v1/chat/completions
```

`--llm-thinking` sets the reasoning effort level: `off`, `low`, `medium`, `high`.
When absent, the provider default applies.

All flags may appear anywhere in the argument list (same as `--system-prompt`).
They are independent — any combination may be specified.

### Environment variables

```
AILETS_MODEL=claude-sonnet-4-6
AILETS_LLM_URL=https://api.anthropic.com/v1/messages
AILETS_LLM_THINKING=high
```

Each is resolved before any default but overridden by the corresponding flag.

### Resolution order (highest → lowest priority)

For model:
1. `--model` CLI flag
2. `AILETS_MODEL` environment variable

For URL:
1. `--llm-url` CLI flag
2. `AILETS_LLM_URL` environment variable

For thinking level:
1. `--llm-thinking` CLI flag
2. `AILETS_LLM_THINKING` environment variable

The CLI always initializes `$ailets_model`, `$ailets_llm_url`, and
`$ailets_llm_thinking` — to the resolved value, or to an empty string if
neither flag nor env var is set. The request generator will fail at runtime
if model or URL are empty.


## What changes

### `shell_ui.rs` — argument parsing

Add `model: Option<String>`, `llm_url: Option<String>`, and
`llm_thinking: Option<String>` to `CliArgs`.

Parse `--model`, `--llm-url`, and `--llm-thinking` in `parse_args`. Resolve
short aliases for model. If a flag is absent, check the corresponding
`AILETS_*` env var. If that is also absent, leave the field as `None`.

### `DagShell` / TCL environment

Expose the resolved values as TCL variables `$ailets_model`,
`$ailets_llm_url`, and `$ailets_llm_thinking` (empty string when not set):

```tcl
if {$ailets_model ne ""} {
    set_model $ailets_model
}
if {$ailets_llm_url ne ""} {
    set_llm_url $ailets_llm_url
}
if {$ailets_llm_thinking ne ""} {
    set_llm_thinking $ailets_llm_thinking
}
```

### `print_usage` / `command-line.md`

Document the new flags and env vars.

---

## Examples

```bash
# Short alias
dagsh --model sonnet "Explain this code"

# Model with thinking level
dagsh --model opus --llm-thinking high @report.md "Summarize"

# Full model ID with explicit endpoint
dagsh --model claude-opus-4-8 --llm-url https://api.anthropic.com/v1/messages @report.md "Summarize"

# Local Ollama endpoint, model from env var
export AILETS_MODEL=llama3
dagsh --llm-url http://localhost:11434/v1/chat/completions -l run.tcl

# Override only the model, let the script supply the URL
export AILETS_MODEL=claude-haiku-4-5-20251001
dagsh -l run.tcl

# Override only the URL (route through a proxy), keep script's model
dagsh --llm-url https://my-proxy.example.com/v1/chat/completions -l run.tcl

# CI: full override via env vars
export AILETS_MODEL=claude-sonnet-4-6
export AILETS_LLM_URL=https://api.anthropic.com/v1/messages
export AILETS_LLM_THINKING=low
dagsh -l run.tcl
```
