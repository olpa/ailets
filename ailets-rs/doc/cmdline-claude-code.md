# Claude Code CLI: Complete Prompt Input Reference

## 1. Simple Inline Prompts

```bash
claude "do something"
claude "explain this project"
```

The first positional argument is the initial prompt. In interactive mode, conversation continues after; with `-p` it exits after the response.

---

## 2. Non-Interactive / Print Mode

Essential for scripting — reads stdin and exits after response:

```bash
claude -p "query"
claude --print "query"
```

---

## 3. Piping Stdin

```bash
cat logs.txt | claude -p "analyze these errors"
git diff | claude -p "review these changes"
echo "test content" | claude -p "process this"
```

Stdin is capped at **10MB**. For larger inputs, write to a file and reference the path instead.

---

## 4. File Redirect

```bash
claude < file.txt                    # interactive session
claude -p "process this" < file.txt  # non-interactive
```

---

## 5. Referencing Files by Path in Prompts

Claude reads files you mention by path:

```bash
claude "analyze /path/to/file.py"
claude -p "find bugs in src/auth.js"
```

Works with relative and absolute paths. PDFs are also supported.

---

## 6. Passing Images

**Reference in prompt:**
```bash
claude "analyze this screenshot" path/to/screenshot.png
claude -p "extract text from image" /path/to/photo.jpg
```

**Paste from clipboard** (interactive mode only):
- `Ctrl+V` / `Cmd+V` on macOS iTerm2 / `Alt+V` on Windows/WSL
- Inserts an `[Image #N]` reference you can refer to in your message

Supported formats: PNG, JPG, GIF, WebP.

---

## 7. System Prompt Control

| Flag | Effect |
|------|--------|
| `--system-prompt "text"` | Replace entire default system prompt |
| `--system-prompt-file ./file.txt` | Replace with file contents |
| `--append-system-prompt "text"` | Append to default (preserves Claude Code defaults) |
| `--append-system-prompt-file ./file.txt` | Append file to default |

```bash
# Add per-invocation rules without removing defaults
claude -p "refactor this" --append-system-prompt "Use functional programming patterns"

# Fully replace for a custom agent role
claude -p "process records" --system-prompt "You are a data validation tool"
```

---

## 8. Session Continuation

```bash
claude --continue                      # resume most recent session (interactive)
claude -c                              # short form
claude -c -p "next step"              # resume in non-interactive mode

claude --resume session-id             # resume specific session by ID
claude -r "auth-refactor" "continue"  # resume by name
```

---

## 9. Model Selection

```bash
claude --model sonnet
claude --model opus
claude --model haiku
claude --model claude-sonnet-4-6       # full model ID

# Via environment variable
export ANTHROPIC_MODEL=claude-sonnet-4-6
claude "work"
```

---

## 10. Output Format

```bash
claude -p "query" --output-format json         # structured JSON with metadata
claude -p "query" --output-format stream-json  # newline-delimited streaming JSON
claude -p "query" --verbose                    # show all turns

# Parse with jq
claude -p "query" --output-format json | jq -r '.result'
```

JSON output includes `result`, `session_id`, `usage`, and metadata.

---

## 11. Key Environment Variables

| Variable | Effect |
|----------|--------|
| `ANTHROPIC_API_KEY` | API key (overrides subscription auth) |
| `ANTHROPIC_MODEL` | Default model |
| `ANTHROPIC_BASE_URL` | Route through proxy/gateway |
| `API_TIMEOUT_MS` | Request timeout in ms (default: 600000) |
| `BASH_DEFAULT_TIMEOUT_MS` | Bash tool timeout (default: 120000) |
| `CLAUDE_CODE_SKIP_PROMPT_HISTORY` | Disable session persistence |
| `CLAUDE_CODE_DEBUG_LOGS_DIR` | Debug log output directory |

---

## 12. MCP Server Injection

```bash
# Load MCP servers for this session
claude --mcp-config ./mcp.json

# Inline JSON
claude --mcp-config '{"mcpServers":{"myserver":{"command":"python","args":["server.py"]}}}'

# Only use specified servers (strict mode)
claude --strict-mcp-config --mcp-config ./mcp.json
```

---

## 13. Tool Allow/Deny Lists

```bash
# Allow only specific tools
claude -p "query" --allowedTools "Read,Edit,Bash"

# Deny all MCP tools
claude -p "query" --disallowedTools "mcp__*"
```

---

## 14. Agentic Limits

```bash
claude -p "query" --max-turns 3          # limit agentic iterations
claude -p "query" --max-budget-usd 5.00  # spending cap per run
```

---

## 15. Additional Directories

Grant file access beyond the working directory:

```bash
claude --add-dir ../apps ../lib
```

---

## 16. Combining Everything

```bash
# Pipe + system prompt + output format
git diff | claude -p \
  --append-system-prompt "You are a security reviewer" \
  "Check for vulnerabilities" \
  --output-format json

# Multi-file shell expansion
claude -p "Compare these files" *.ts

# Resume session + image
claude -c -p "explain what changed in this screenshot" shot.png

# Full pipeline with jq
cat build-log.txt | claude -p "What failed?" --output-format json | jq -r '.result'
```

---

## Quick Reference Table

| Input method | Pattern |
|---|---|
| Inline prompt | `claude "text"` |
| Non-interactive | `claude -p "text"` |
| Stdin pipe | `cmd \| claude -p "text"` |
| File redirect | `claude -p "text" < file` |
| File reference | `claude -p "check src/main.py"` |
| Image | `claude -p "analyze" image.png` |
| System prompt replace | `--system-prompt "text"` |
| System prompt append | `--append-system-prompt "text"` |
| From file (system) | `--system-prompt-file ./file.txt` |
| Resume session | `-c` or `--resume id` |
| Model override | `--model sonnet` |
| JSON output | `--output-format json` |
| MCP servers | `--mcp-config ./mcp.json` |
| Extra dirs | `--add-dir ../path` |
