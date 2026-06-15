# Pi Coding Agent CLI: Complete Prompt Input Reference

Source: `/home/olpa/opt/pi/packages/coding-agent/src`

## Usage form

```
pi [options] [@files...] [messages...]
```

---

## 1. Simple Inline Prompts (Interactive TUI)

```bash
pi "List all .ts files in src/"
pi "Help me refactor the auth module"
```

The first positional argument (not starting with `@` or `-`) becomes the initial message. Launches the interactive TUI with it pre-submitted.

---

## 2. Multiple Messages in One Invocation

```bash
pi "Read package.json" "What dependencies do we have?"
```

Multiple positional arguments are treated as a sequence of turns. The first is sent as the initial message; the rest are sent as follow-up prompts after the agent finishes each response.

---

## 3. Non-Interactive / Print Mode

```bash
pi -p "Summarize this codebase"
pi --print "Summarize this codebase"
```

Processes the prompt and exits. Output is the final assistant response text on stdout.

**Auto-detection:** If either stdin or stdout is not a TTY (i.e. piped), pi automatically uses print mode even without `-p`.

---

## 4. Piping Stdin

```bash
cat README.md | pi -p "Summarize this text"
git diff | pi -p "Review these changes"
echo "context" | pi -p "Process this"
```

When stdin is piped (not a TTY), its content is read and prepended to the initial message. The assembled prompt is: `stdinContent + @fileContent + firstPositionalArg`.

Empty/whitespace-only stdin is ignored.

---

## 5. `@file` Arguments — Include Files in the Prompt

Prefix any file path with `@` to include it in the initial message:

```bash
pi @prompt.md "Answer this"
pi -p @screenshot.png "What's in this image?"
pi @code.ts @test.ts "Review these files"
pi @context.md @diagram.png "Explain the architecture"
```

**Text files** are included as `<file name="...">content</file>` blocks.

**Image files** (PNG, JPG, GIF, WebP, etc.) are detected by MIME type, embedded as base64, auto-resized to 2000×2000 max, and attached alongside a `<file name="...">WxH px</file>` reference in the text.

Multiple `@file` args are processed in order; all text is concatenated, all images are collected as a list.

**Combined with stdin:**
```bash
cat context.md | pi -p @diagram.png "Explain using the context and diagram"
# Result: stdin text + diagram image ref + positional message
```

---

## 6. System Prompt Control

```bash
# Replace the default system prompt entirely
pi --system-prompt "You are a Python expert. Only use Python."

# Append to the default system prompt (can be used multiple times)
pi --append-system-prompt "Always include type annotations."
pi --append-system-prompt "Rule 1" --append-system-prompt "Rule 2"
```

When `--system-prompt` is used, context files and skills are **still appended** after the replacement — only the built-in coding-assistant preamble is replaced.

### Context Files (project-level instructions)

Pi auto-discovers and appends these to the system prompt (appended after `--system-prompt` or the default):

| File | Location | Effect |
|------|----------|--------|
| `AGENTS.md` or `CLAUDE.md` | `~/.pi/agent/`, parent dirs, current dir | Appended as project instructions |
| `.pi/SYSTEM.md` | Project root or `~/.pi/agent/` | **Replaces** the default system prompt |
| `APPEND_SYSTEM.md` | Project root or `~/.pi/agent/` | Appended to the default system prompt |

Disable context file loading:
```bash
pi --no-context-files   # or -nc
```

---

## 7. Output Modes

| Flag | Output |
|------|--------|
| *(default)* | Interactive TUI |
| `-p` / `--print` | Final assistant text on stdout, then exit |
| `--mode json` | All session events as JSON lines on stdout, then exit |
| `--mode rpc` | Bidirectional JSON RPC over stdin/stdout (persistent process) |

### JSON mode (`--mode json`)

```bash
pi --mode json "summarize this repo"
cat file.txt | pi --mode json "analyze"
```

Emits every session event as a JSON line. The first line is a session header. Useful for programmatic processing.

### RPC mode (`--mode rpc`)

Starts pi as a long-running process. Send JSON commands on stdin, receive events and responses on stdout — both as JSON lines.

Key RPC commands:
```json
{"type": "prompt", "message": "fix the bug"}
{"type": "prompt", "message": "analyze this", "images": [...]}
{"type": "steer", "message": "actually use TypeScript"}
{"type": "follow_up", "message": "now run the tests"}
{"type": "abort"}
{"type": "set_model", "provider": "anthropic", "modelId": "claude-opus-4-8"}
{"type": "set_thinking_level", "level": "high"}
{"type": "compact", "customInstructions": "focus on auth"}
{"type": "get_state"}
{"type": "get_messages"}
{"type": "export_html", "outputPath": "/tmp/session.html"}
```

---

## 8. Session Management

```bash
pi -c                           # Continue most recent session
pi --continue "next step"       # Continue with a follow-up prompt

pi -r                           # Browse and pick a session to resume
pi --resume

pi --session <path|id>          # Use specific session file or partial UUID
pi --session-id <id>            # Use exact project session ID, creating if missing
pi --fork <path|id>             # Fork a session into a new file

pi --no-session "one-off task"  # Ephemeral: do not save session
pi --name "release audit"       # Set session display name at startup
pi -n "auth refactor"           # Short form

pi --session-dir /path/to/dir   # Custom session storage directory
```

Sessions are saved to `~/.pi/agent/sessions/`, organized by working directory.

---

## 9. Model Selection

```bash
# Provider + model separately
pi --provider anthropic --model claude-sonnet-4-6 "..."
pi --provider openai --model gpt-4o "..."
pi --provider google --model gemini-2.5-pro "..."

# Provider/model shorthand (no --provider needed)
pi --model anthropic/claude-sonnet-4-6 "..."
pi --model openai/gpt-4o "..."

# Fuzzy/pattern matching
pi --model sonnet "..."
pi --model gpt-4o-mini "..."

# Model with thinking level embedded
pi --model sonnet:high "Solve this hard problem"
pi --model haiku:off "Quick task"

# Thinking level separately
pi --thinking high "..."
pi --thinking off "..."
# Levels: off, minimal, low, medium, high, xhigh

# API key override (takes precedence over env vars)
pi --api-key sk-... "..."

# Models available for Ctrl+P cycling (comma-separated patterns)
pi --models "claude-*,gpt-4o"
pi --models "sonnet:high,haiku:low"
pi --models "github-copilot/*"

# List available models
pi --list-models
pi --list-models "sonnet"
```

---

## 10. Tool Control

Built-in tools: `read`, `bash`, `edit`, `write`, `grep`, `find`, `ls`  
(`grep`, `find`, `ls` are off by default; enabled via `--tools`.)

```bash
# Allow only specific tools
pi --tools read,grep,find,ls -p "Review the code"
pi -t read,bash "..."

# Disable one or more tools while keeping the rest
pi --exclude-tools ask_question
pi -xt bash,write "Read-only review"

# Disable all built-in tools (extension/custom tools still active)
pi --no-builtin-tools
pi -nbt

# Disable all tools
pi --no-tools
pi -nt
```

---

## 11. Extensions, Skills, Prompt Templates, Themes

```bash
# Load a specific extension (repeatable)
pi --extension ./my-extension.ts "..."
pi -e ./my-extension.ts -e npm:some-package "..."

# Disable extension auto-discovery (explicit -e paths still work)
pi --no-extensions -e ./my-extension.ts

# Load a skill file or directory (repeatable)
pi --skill ./skills/my-skill.md "..."

# Disable skill auto-discovery
pi --no-skills

# Load a prompt template
pi --prompt-template ./templates/review.md

# Disable prompt template auto-discovery
pi --no-prompt-templates   # or -np

# Load a theme
pi --theme ./themes/dark.json

# Disable theme auto-discovery
pi --no-themes
```

Extensions can also register custom CLI flags (shown in `pi --help`):
```bash
# Example: plan-mode extension adds --plan
pi --plan "Implement OAuth login"
```

---

## 12. Interactive-Only Input Features

These only work in the interactive TUI:

| Feature | How |
|---------|-----|
| File fuzzy search | Type `@` in the editor to search project files |
| Path completion | Press `Tab` to complete paths |
| Paste image | `Ctrl+V` / `Alt+V` (Windows) / drag into terminal |
| Shell command (sends output to model) | `!command` in the editor |
| Shell command (hidden from model) | `!!command` in the editor |
| External editor | `Ctrl+G` opens `$VISUAL` or `$EDITOR` |
| Multi-line | `Shift+Enter` (or `Ctrl+Enter` on Windows Terminal) |

---

## 13. Project Trust

Non-interactive modes (`-p`, `--mode json`, `--mode rpc`) skip the trust prompt and use `defaultProjectTrust` from global settings.

```bash
pi --approve     # or -a   Trust project-local files for this run
pi --no-approve  # or -na  Ignore project-local files for this run
```

---

## 14. Key Environment Variables

### Provider API Keys

| Variable | Provider |
|----------|----------|
| `ANTHROPIC_API_KEY` | Anthropic Claude |
| `ANTHROPIC_OAUTH_TOKEN` | Anthropic (OAuth alternative) |
| `OPENAI_API_KEY` | OpenAI |
| `AZURE_OPENAI_API_KEY` | Azure OpenAI |
| `AZURE_OPENAI_BASE_URL` | Azure OpenAI base URL |
| `AZURE_OPENAI_RESOURCE_NAME` | Azure OpenAI resource name |
| `AZURE_OPENAI_API_VERSION` | Azure OpenAI API version (default: v1) |
| `AZURE_OPENAI_DEPLOYMENT_NAME_MAP` | Model→deployment map (comma-separated) |
| `GEMINI_API_KEY` | Google Gemini |
| `DEEPSEEK_API_KEY` | DeepSeek |
| `NVIDIA_API_KEY` | NVIDIA NIM |
| `GROQ_API_KEY` | Groq |
| `CEREBRAS_API_KEY` | Cerebras |
| `XAI_API_KEY` | xAI Grok |
| `FIREWORKS_API_KEY` | Fireworks |
| `TOGETHER_API_KEY` | Together AI |
| `OPENROUTER_API_KEY` | OpenRouter |
| `MISTRAL_API_KEY` | Mistral |
| `MINIMAX_API_KEY` | MiniMax |
| `MOONSHOT_API_KEY` | Moonshot AI |
| `CLOUDFLARE_API_KEY` | Cloudflare Workers AI / AI Gateway |
| `CLOUDFLARE_ACCOUNT_ID` | Cloudflare account ID |
| `CLOUDFLARE_GATEWAY_ID` | Cloudflare AI Gateway slug |
| `AWS_PROFILE` / `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` | Amazon Bedrock |
| `AWS_BEARER_TOKEN_BEDROCK` | Bedrock bearer token |
| `AWS_REGION` | AWS region for Bedrock |

### Pi-Specific Variables

| Variable | Effect |
|----------|--------|
| `PI_CODING_AGENT_DIR` | Override config dir (default: `~/.pi/agent`) |
| `PI_CODING_AGENT_SESSION_DIR` | Override session storage dir; overridden by `--session-dir` |
| `PI_PACKAGE_DIR` | Override package directory (for Nix/Guix store paths) |
| `PI_OFFLINE` | Set to `1`/`true`/`yes` to disable startup network ops |
| `PI_SKIP_VERSION_CHECK` | Skip version update check at startup |
| `PI_TELEMETRY` | Override install/update telemetry (`1` or `0`) |
| `PI_CACHE_RETENTION` | Set to `long` for extended prompt cache where supported |
| `PI_SHARE_VIEWER_URL` | Base URL for `/share` command |
| `VISUAL` / `EDITOR` | External editor opened by `Ctrl+G` |

---

## 15. Other Flags

```bash
pi --export session.jsonl           # Export session file to HTML (stdout)
pi --export session.jsonl out.html  # Export to a specific file

pi --list-models                    # List available models
pi --list-models "sonnet"           # Fuzzy search models

pi --verbose                        # Force verbose startup output
pi --offline                        # Same as PI_OFFLINE=1

pi --help    # or -h
pi --version # or -v
```

---

## 16. `~/.pi/agent/settings.json` Key Fields

```json
{
  "defaultProjectTrust": "ask",     // "ask" | "always" | "never"
  "quietStartup": false,
  "steeringMode": "all",            // message queue behavior
  "followUpMode": "all"
}
```

---

## Quick Reference Table

| Input method | Pattern |
|---|---|
| Inline prompt (interactive) | `pi "text"` |
| Multiple sequential messages | `pi "msg1" "msg2"` |
| Non-interactive (print) | `pi -p "text"` |
| Stdin pipe | `cmd \| pi -p "text"` |
| Include text file | `pi @file.txt "text"` |
| Include image file | `pi -p @image.png "describe"` |
| Multiple files | `pi @a.ts @b.ts "review"` |
| Stdin + file + prompt | `cat ctx.md \| pi -p @diagram.png "explain"` |
| System prompt replace | `--system-prompt "text"` |
| System prompt append | `--append-system-prompt "text"` (repeatable) |
| Project instructions | `AGENTS.md` / `CLAUDE.md` in project / home |
| Project system prompt | `.pi/SYSTEM.md` |
| Project system append | `APPEND_SYSTEM.md` |
| Model selection | `--model sonnet:high` or `--provider anthropic --model ...` |
| Thinking level | `--thinking high` |
| JSON event stream | `--mode json "text"` |
| RPC mode | `--mode rpc` (long-running, JSON over stdin/stdout) |
| Continue session | `-c` or `--continue` |
| Resume session | `-r` or `--session <id>` |
| Fork session | `--fork <id>` |
| Ephemeral | `--no-session` |
| Allow only tools | `--tools read,grep,find` |
| Disable tool | `--exclude-tools bash` |
| Load extension | `-e ./ext.ts` |
| Load skill | `--skill ./skill.md` |
| Disable context files | `--no-context-files` / `-nc` |
| Trust project | `--approve` / `-a` |
| Export to HTML | `--export session.jsonl out.html` |
