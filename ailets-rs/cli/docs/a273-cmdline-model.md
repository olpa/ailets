# A273: LLM Model and Endpoint Selection on the Command Line

## Goal

Allow users to select the LLM model, provider endpoint, and thinking level
without editing TCL scripts or DAG definitions — through CLI flags and through
environment variables, in that priority order.

---

## Supported providers

All providers use the OpenAI-compatible `/v1/chat/completions` endpoint and
the same actor pipeline (`messages_to_query` + `gpt.response_to_messages`).

| Provider | URL |
|----------|-----|
| OpenAI (primary, default) | `https://api.openai.com/v1/chat/completions` |
| Anthropic | `https://api.anthropic.com/v1/chat/completions` |
| Google | `https://generativelanguage.googleapis.com/v1beta/openai/chat/completions` |
| Ollama (local) | `http://localhost:11434/v1/chat/completions` |

---

## Model aliases

Each alias resolves to a full model ID and the provider URL. Specifying
`--llm-url` explicitly overrides the URL derived from an alias.

| Alias | Model ID | URL |
|-------|----------|-----|
| `gpt` | `gpt-5.4` | `https://api.openai.com/v1/chat/completions` |
| `gpt-mini` | `gpt-5.4-mini` | `https://api.openai.com/v1/chat/completions` |
| `fable` | `claude-fable-5` | `https://api.anthropic.com/v1/chat/completions` |
| `opus` | `claude-opus-4-8` | `https://api.anthropic.com/v1/chat/completions` |
| `sonnet` | `claude-sonnet-4-6` | `https://api.anthropic.com/v1/chat/completions` |
| `haiku` | `claude-haiku-4-5` | `https://api.anthropic.com/v1/chat/completions` |
| `gemini` | `gemini-2.5-flash` | `https://generativelanguage.googleapis.com/v1beta/openai/chat/completions` |
| `flash` | `gemini-3.5-flash` | `https://generativelanguage.googleapis.com/v1beta/openai/chat/completions` |
| `local` | _(user-defined)_ | `http://localhost:11434/v1/chat/completions` |

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
| Short alias | `sonnet`, `opus`, `haiku`, `gpt`, `gemini` | Resolves to both a full model ID and the provider URL |
| Full model ID | `claude-sonnet-4-6` | Passed verbatim; URL must be set separately |

Resolving a short alias sets **both** `AILETS_MODEL` and `AILETS_LLM_URL`
unless `--llm-url` is explicitly provided (explicit flag wins).

`--llm-url` accepts the full endpoint URL:

```
--llm-url https://api.anthropic.com/v1/chat/completions
--llm-url http://localhost:11434/v1/chat/completions
```

`--llm-thinking` sets the reasoning effort level: `off`, `low`, `medium`, `high`.
When absent, the provider default applies.

All flags may appear anywhere in the argument list (same as `--system-prompt`).
They are independent — any combination may be specified.

### Environment variables

```
AILETS_MODEL=claude-sonnet-4-6
AILETS_LLM_URL=https://api.anthropic.com/v1/chat/completions
AILETS_LLM_THINKING=high
```

Each is resolved before any default but overridden by the corresponding flag.

### Resolution order (highest → lowest priority)

For model:
1. `--model` CLI flag
2. `AILETS_MODEL` environment variable

For URL:
1. `--llm-url` CLI flag
2. URL implied by `--model` alias
3. `AILETS_LLM_URL` environment variable

For thinking level:
1. `--llm-thinking` CLI flag
2. `AILETS_LLM_THINKING` environment variable

When neither flag, alias, nor env var provides a value, the actor falls back
to OpenAI-safe defaults (`gpt-4o-mini` / `api.openai.com`).

---

## Internal architecture

Values are stored in `EnvService` (an ailetos service owned by `Environment`
and shared with every `BlockingActorRuntime`). Actors read them via
`runtime.get_env(key)` — a method on the `ActorRuntime` trait.

The `messages_to_query` actor reads `AILETS_MODEL`, `AILETS_LLM_URL`, and
`AILETS_LLM_THINKING` directly from the runtime in `write_prologue`.

The CLI populates `EnvService` before launching actors.

---

## What changes

### `shell_ui.rs` — argument parsing

Add `model: Option<String>`, `llm_url: Option<String>`, and
`llm_thinking: Option<String>` to `CliArgs`.

Parse `--model`, `--llm-url`, and `--llm-thinking` in `parse_args`. Resolve
short aliases for model (and derive URL from alias when not overridden by
`--llm-url`). If a flag is absent, check the corresponding `AILETS_*` env var.

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
# Short alias — sets both model and URL automatically
dagsh --model sonnet "Explain this code"

# Model with thinking level
dagsh --model opus --llm-thinking high @report.md "Summarize"

# Full model ID with explicit endpoint
dagsh --model claude-opus-4-8 --llm-url https://api.anthropic.com/v1/chat/completions @report.md "Summarize"

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
export AILETS_LLM_URL=https://api.anthropic.com/v1/chat/completions
export AILETS_LLM_THINKING=low
dagsh -l run.tcl
```
