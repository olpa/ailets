# OpenAI Codex CLI: Complete Prompt Input Reference

Source: https://github.com/openai/codex (codex-rs, the Rust rewrite)

## Usage forms

```
codex [OPTIONS] [PROMPT]
codex [OPTIONS] <COMMAND> [ARGS]
```

---

## 1. Simple Inline Prompts (Interactive TUI)

```bash
codex "fix the bug in auth.rs"
codex "explain how this project works"
```

Launches the interactive TUI with the given string as the opening prompt. Conversation continues interactively after the first response.

---

## 2. Non-Interactive Mode: `codex exec`

`exec` (alias `e`) runs Codex headlessly and exits after the agent finishes:

```bash
codex exec "add unit tests for the parser"
codex e "refactor to use async/await"
```

### Stdin as prompt

When no positional `PROMPT` is given and stdin is piped, stdin becomes the prompt:

```bash
cat spec.md | codex exec
```

When both a positional prompt and piped stdin are provided, stdin is appended as a `<stdin>` block after the prompt:

```bash
cat context.md | codex exec "Summarize this document"
```

Use `-` as the prompt to force stdin to be read as the primary prompt regardless:

```bash
cat prompt.txt | codex exec -
```

### Output options

```bash
# Print events as JSONL to stdout
codex exec --json "run the tests"

# Also available as alias:
codex exec --experimental-json "..."

# Write the agent's last message to a file
codex exec -o /tmp/result.txt "summarize the repo"
codex exec --output-last-message /tmp/result.txt "..."
```

### Structured output (JSON Schema)

```bash
# Constrain the model's final response to a JSON schema
codex exec --output-schema ./response-schema.json "extract function signatures"
```

---

## 3. Passing Images

```bash
# Attach one or more images to the initial prompt
codex --image screenshot.png "what is wrong with this UI?"
codex exec -i diagram.png,schema.png "explain the architecture"

# Short form, comma-separated or repeated
codex -i img1.png -i img2.png "compare these"
```

The `--image` / `-i` flag accepts paths to local image files. Multiple images can be passed by repeating the flag or using a comma-separated list.

---

## 4. Referencing Files in Prompts

Codex can read files you mention by path in the prompt. It also automatically loads `AGENTS.md` (or files listed in `project_doc_fallback_filenames`) from the project root as context:

```bash
codex exec "look at src/auth.rs and fix the login bug"
codex exec "review the changes in src/ and write a summary"
```

There is no special flag — just mention the path. Codex's agent tools handle reading the file.

---

## 5. System Prompt / Instructions

Instructions are set via config, not a CLI flag.

**`~/.codex/config.toml`:**
```toml
# Replace the built-in system instructions
instructions = "You are a senior Rust engineer. Be concise."

# Insert a developer-role message (shown to model but not user)
developer_instructions = "Always prefer iterators over manual loops."
```

**Per-session override via `-c`:**
```bash
codex exec -c 'instructions="You are a Python expert."' "rewrite this in Python"
```

**Project-level (`AGENTS.md` / `CODEX.md`):**
Codex automatically reads `AGENTS.md` at the project root (up to `project_doc_max_bytes`, default 32KB) as project-level instructions. The filename list is configurable:
```toml
project_doc_fallback_filenames = ["CODEX.md", ".instructions.md"]
```

---

## 6. Session Management

### Resume

```bash
# Pick a session from a list (interactive picker)
codex resume

# Resume the most recent session
codex resume --last

# Resume specific session by UUID or name
codex resume <session-id-or-name>

# Resume and send an additional prompt
codex resume --last "continue where you left off"

# Resume with attached images
codex resume --last -i screenshot.png "fix the issue shown here"

# Show all sessions (not filtered to current directory)
codex resume --all

# Include non-interactive sessions in picker
codex resume --include-non-interactive

# Also works as a subcommand of exec:
codex exec resume --last "next step"
```

### Fork

```bash
# Fork a session (creates a new branch from an existing one)
codex fork
codex fork --last
codex fork <session-id>
```

### Archive / Delete

```bash
codex archive <session-id-or-name>
codex unarchive <session-id-or-name>
codex delete <session-id-or-name>
codex delete --force <uuid>   # skip confirmation, UUID only
```

### Ephemeral sessions

```bash
# Run without persisting session files to disk
codex exec --ephemeral "one-off task"
```

---

## 7. Model Selection

```bash
# CLI flag
codex --model o3 "..."
codex exec -m codex-mini-latest "..."

# Via config
# ~/.codex/config.toml
# model = "o3"

# Via -c override
codex exec -c 'model="o3"' "..."

# Use a local/OSS provider (LM Studio or Ollama)
codex --oss "..."
codex --local-provider lmstudio "..."
codex --local-provider ollama "..."
```

Reasoning effort (for models that support it):
```toml
# config.toml
model_reasoning_effort = "high"   # low | medium | high
```

---

## 8. Sandbox / Approval Policy

### Sandbox mode (`--sandbox` / `-s`)

Controls what the agent's shell commands are allowed to do:

```bash
codex exec -s read-only "analyze the codebase"
codex exec -s workspace-write "refactor the auth module"
codex exec -s danger-full-access "..."   # no restrictions

# Skip all approval prompts AND sandbox (DANGEROUS)
codex exec --dangerously-bypass-approvals-and-sandbox "..."
codex exec --yolo "..."   # alias
```

### Approval policy (`--ask-for-approval` / `-a`) — interactive TUI only

```bash
codex --ask-for-approval untrusted "..."   # ask unless command is trusted
codex -a on-request "..."                  # model decides when to ask
codex -a never "..."                       # never ask
codex -a on-failure "..."                  # (deprecated) ask only on failure
```

Via config:
```toml
approval_policy = "on-request"   # untrusted | on-failure | on-request | never
```

---

## 9. Configuration Overrides (`-c`)

Override any config.toml field at the command line using dotted TOML paths:

```bash
codex exec -c 'model="o3"' "..."
codex exec -c 'sandbox_mode="workspace-write"' "..."
codex exec -c 'approval_policy="never"' "..."
codex exec -c 'sandbox_permissions=["disk-full-read-access"]' "..."
codex exec -c shell_environment_policy.inherit=all "..."

# Multiple overrides
codex exec -c 'model="o3"' -c 'approval_policy="never"' "..."
```

Values are parsed as TOML; if TOML parsing fails the value is used as a raw string.

---

## 10. Config Profiles

Layer an additional config file on top of the base user config:

```bash
# Loads ~/.codex/<name>.config.toml on top of ~/.codex/config.toml
codex --profile strict "..."
codex exec -p research "..."
```

Named profiles can also be defined inside `config.toml`:
```toml
[profiles.strict]
approval_policy = "untrusted"
sandbox_mode = "read-only"
```

---

## 11. Web Search

```bash
# Enable live web search (interactive TUI only)
codex --search "what is the latest Rust edition?"
```

Or in config:
```toml
# config.toml
[web_search]
enabled = true
```

---

## 12. Code Review (Non-Interactive)

```bash
# Review uncommitted changes
codex exec review --uncommitted

# Review changes vs a base branch
codex exec review --base main

# Review a specific commit
codex exec review --commit abc1234
codex exec review --commit abc1234 --title "Add login flow"

# Custom review instructions
codex exec review --base main "Focus on security issues"

# Read custom instructions from stdin
codex exec review --base main -   # reads from stdin
```

Also available as top-level subcommand:
```bash
codex review --uncommitted
codex review --base main "check for race conditions"
```

---

## 13. Working Directory and Additional Dirs

```bash
# Set the working root for the agent
codex --cd /path/to/project "..."
codex exec -C /path/to/project "..."

# Allow writes to additional directories beyond the workspace
codex exec --add-dir ../shared-lib ../docs "update the docs"
```

---

## 14. Key Environment Variables

| Variable | Effect |
|----------|--------|
| `OPENAI_API_KEY` | OpenAI API key for direct API access |
| `CODEX_API_KEY` | Alternative API key (checked after `OPENAI_API_KEY`) |
| `CODEX_ACCESS_TOKEN` | ChatGPT OAuth access token |
| `CODEX_HOME` | Override the Codex home directory (default: `~/.codex`) |
| `CODEX_SQLITE_HOME` | Override where SQLite state DB is stored |
| `RUST_LOG` | Control log verbosity (e.g. `RUST_LOG=debug`) |

---

## 15. `~/.codex/config.toml` Key Fields

```toml
# Model selection
model = "o3"
model_provider = "openai"
model_reasoning_effort = "high"   # low | medium | high

# Instructions (system prompt equivalent)
instructions = "You are a senior engineer. Be concise."
developer_instructions = "Prefer iterators over manual loops."

# Approval and sandbox
approval_policy = "on-request"    # untrusted | on-failure | on-request | never
sandbox_mode = "workspace-write"  # read-only | workspace-write | danger-full-access

# Project docs
project_doc_max_bytes = 32768
project_doc_fallback_filenames = ["CODEX.md"]

# MCP servers
[mcp_servers.my-server]
command = "python"
args = ["server.py"]

# Named permission profiles
[permissions.my-profile]
# ...

# Named config profiles
[profiles.strict]
approval_policy = "untrusted"
```

Config is looked up in order (later overrides earlier):
1. Built-in defaults
2. `~/.codex/config.toml` (unless `--ignore-user-config`)
3. Project config (`AGENTS.md` / project doc)
4. `--profile <name>` layer (`~/.codex/<name>.config.toml`)
5. `-c key=value` CLI overrides

---

## 16. MCP Servers

Configured via `~/.codex/config.toml`:
```toml
[mcp_servers.github]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]

[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
```

Manage MCP servers with the `mcp` subcommand:
```bash
codex mcp list
codex mcp add <name>
codex mcp remove <name>
```

---

## 17. Other Useful Flags

```bash
# Allow running outside a git repository
codex exec --skip-git-repo-check "..."

# Do not load user config (auth still uses CODEX_HOME)
codex exec --ignore-user-config "..."

# Do not load user/project execpolicy .rules files
codex exec --ignore-rules "..."

# Error on unrecognized config.toml fields
codex exec --strict-config "..."

# Disable alternate screen (inline mode, keeps terminal scrollback)
codex --no-alt-screen "..."

# Color control
codex exec --color always "..."
codex exec --color never "..."
codex exec --color auto "..."   # default

# Shell completions
codex completion bash
codex completion zsh
codex completion fish

# Diagnose installation, config, auth
codex doctor

# Update to latest version
codex update
```

---

## Quick Reference Table

| Input method | Pattern |
|---|---|
| Inline prompt (interactive) | `codex "text"` |
| Non-interactive | `codex exec "text"` |
| Stdin as prompt | `cmd \| codex exec` |
| Force stdin as prompt | `cmd \| codex exec -` |
| Stdin appended to prompt | `cmd \| codex exec "prompt"` |
| Image attachment | `codex exec -i img.png "..."` |
| File reference | mention path in prompt text |
| System prompt | `instructions = "..."` in config.toml |
| Config override | `codex exec -c 'key=value' "..."` |
| Model selection | `codex exec -m o3 "..."` |
| Sandbox mode | `codex exec -s workspace-write "..."` |
| Approval policy | `codex -a never "..."` (TUI only) |
| Resume session | `codex resume --last` |
| Fork session | `codex fork --last` |
| Profile | `codex exec -p strict "..."` |
| Code review | `codex exec review --base main` |
| JSON output | `codex exec --json "..."` |
| Save last message | `codex exec -o result.txt "..."` |
| Structured output | `codex exec --output-schema schema.json "..."` |
| Extra writable dirs | `codex exec --add-dir ../lib "..."` |
| Change working dir | `codex exec -C /path "..."` |
