# DAG Shell Design Document

**Working name:** DAG Shell
**Status:** Draft

## Goals

### Primary
- **Conference demos** - Interactive tool to demonstrate DAG execution at conferences
- **Validation** - Test and validate implementation of corner cases
- **LLM playground** - Experiment with LLM pipelines interactively

### Secondary
- **Learning tool** - Help newcomers understand the DAG execution model
- **Debugging aid** - Inspect and troubleshoot real pipelines
- **Reproducibility** - Save and replay scenarios for bug reports

## Design Decisions

- **Interface:** REPL with split-screen TUI
- **Syntax:** Shell-like commands
- **Scripting:** Supported for automation and demos
- **Foundation:** Built on `ailetos` crate
- **Save format:** TOML

### Screen Layout

```
┌─────────────────────────────────────────────────────────┐
│  [auto-scroll area]                                     │
│  Async output, log messages, command history            │
│  Colored, short-annotated                               │
│  ↑ older commands from CLI scroll up here               │
│                                                         │
│  12:01:02 [node 2] ⚙ running                            │
│  12:01:02 [node 2] output: Hello, world!                │
│  12:01:03 [node 2] ✓ terminated                         │
│  > node add stdin --explain="Read from stdin"           │
│  Added node 1: stdin                                    │
├─────────────────────────────────────────────────────────┤
│  dagsh> _                                               │  ← CLI input
│  [status bar: node count, running, etc.]                │  ← optional
└─────────────────────────────────────────────────────────┘
```

- CLI input always at bottom (last 2-3 lines)
- Auto-scrolling area above shows async events and older commands
- No blocking for long-running ops or `tail -f` - output streams to scroll area
- Ctrl+C cancels current streaming, not the whole shell

## Node Types (from ailetos)

### NodeKind
- **Concrete** - Actual processing node that executes an actor
- **Alias** - Virtual reference to other nodes

### Node Creation Patterns
- **Value nodes** - Output constant data, immediately `Terminated` (`add_value_node`)
- **Actor nodes** - Execute registered actor functions (`add_node`)
- **Alias nodes** - Virtual nodes referencing other nodes (`add_alias`)

### NodeState
- `NotStarted` - Initial state (displayed as `⋯ not built`)
- `Running` - Currently executing (displayed as `⚙ running`)
- `Terminating` - Shutting down (displayed as `⏳ terminating`)
- `Terminated` - Completed (displayed as `✓ built`)

## Command Reference

### Node Management

```
node add <actor> [--explain=text]           # Add actor node
node value <data> [--explain=text]          # Add value node (constant data)
node alias <name> <target>                  # Add alias node
node list                                   # List all nodes with status
node info <handle>                          # Show node details
dep <node> <dependency>                     # Add dependency (node depends on dependency)
deps <node>                                 # Show direct dependencies
rdeps <node>                                # Show direct dependents
```

### I/O Operations

```
cat <node>                          # Show full output of a node
tail <node>                         # Show last N lines of output
tail -f <node>                      # Follow output (streams to scroll area, Ctrl+C to stop)
head <node>                         # Show first N lines of output
write <node> <data>                 # Write data to node input
write <node> << EOF                 # Heredoc-style input
pipe <from> <to>                    # Show pipe contents between nodes
pipe list                           # List all pipes with sizes
buffers                             # Show all buffer states
buffers <node>                      # Show buffer contents for node
```

### Visualization

Uses `Dag::dump()` / `Dag::dump_colored()` from ailetos.

```
show <node>                         # Tree view from node (uses dump_colored)
show <node> --no-color              # Tree view without ANSI colors
```

Example output:
```
.end [⋯ not built]
└── Copy.baz [⋯ not built]
    └── Copy.bar [⋯ not built]
        ├── Static text [✓ built]
        └── Copy.foo [⋯ not built]
            └── Read from stdin [⚙ running]
```

### Execution Control

```
run                                 # Run the DAG
run <node>                          # Run specific node
step                                # Execute one node
pause                               # Pause execution
resume                              # Resume execution
reset                               # Reset DAG state
break <node>                        # Set breakpoint
break list                          # List breakpoints
break rm <node>                     # Remove breakpoint
```

### State Inspection

```
status                              # Overall DAG status
status <node>                       # Node status (NotStarted/Running/Terminating/Terminated)
blocked                             # Show blocked nodes and why
```

### Persistence

```
save <file>                         # Save DAG definition (TOML format)
load <file>                         # Load DAG definition
export <file>                       # Export session transcript
replay <file>                       # Replay recorded session
```

Example TOML save format:
```toml
[[nodes]]
id = 1
actor = "stdin"
explain = "Read from stdin"

[[nodes]]
id = 2
actor = "cat"
explain = "Copy.foo"

[[deps]]
node = 2
depends_on = 1

[[aliases]]
name = ".end"
target = 2
```

### Testing & Validation

```
inject error <node>                 # Inject error at node
inject delay <node> <ms>            # Inject delay
assert <node> contains <pattern>    # Assert output contains pattern
assert <node> matches <regex>       # Assert output matches regex
fuzz <node> [seed]                  # Send random inputs
```

### LLM-Specific

```
model list                          # List available models
model use <name>                    # Switch model
model info                          # Show current model info
tokens                              # Show token usage
compare <node1> <node2>             # Compare outputs side-by-side
```

### Session Management

```
help [command]                      # Show help
history                             # Show command history
source <file>                       # Run script file
alias <name> <command>              # Create command alias
set <option> <value>                # Set configuration
quit                                # Exit
```

## Example Session

```
$ dagsh
dagsh> node add stdin --explain="Read from stdin"
Added node 1: stdin
dagsh> node add cat --explain="Copy.foo"
Added node 2: cat
dagsh> dep 2 1
Added dependency: 2 depends on 1
dagsh> node alias .end 2
Added alias 3: .end -> 2
dagsh> show 3
.end [⋯ not built]
└── Copy.foo [⋯ not built]
    └── Read from stdin [⋯ not built]
dagsh> run 3
dagsh> write 1 "Hello, world!"
dagsh> tail -f 2
Hello, world!
^C
dagsh> status
Node 1 (stdin): Terminated
Node 2 (cat): Terminated
Node 3 (.end): Alias -> 2
dagsh> save my-pipeline.dag
Saved to my-pipeline.dag
```

## Implementation Notes

### Architecture

```
┌─────────────────────────────────────────┐
│              DAG Shell (dagsh)          │
├─────────────────────────────────────────┤
│  TUI Layer (ratatui/crossterm)          │
│    ├── Scroll area (async output)       │
│    ├── CLI input area                   │
│    └── Optional status bar              │
├─────────────────────────────────────────┤
│  Command Layer                          │
│    ├── Parser (shell-like syntax)       │
│    ├── Command Dispatcher               │
│    └── Output Formatter                 │
├─────────────────────────────────────────┤
│  ailetos crate                          │
│    ├── Environment - high-level API     │
│    ├── Dag - node/dependency management │
│    ├── Dag::dump_colored() - viz        │
│    └── Actors - stdin, cat, etc.        │
└─────────────────────────────────────────┘
```

### Dependencies

- `ailetos` - DAG runtime, nodes, visualization
- `ratatui` or `crossterm` - split-screen TUI with scroll area
- `rustyline` or custom input handling - line editing with history

### Defaults

**Scroll area history:** 1000 lines (configurable via `set history_size <n>`)

**Status bar content:**
```
[nodes: 3 total, 1 running, 2 done] [buffers: 2.1 KB] [12:01:03]
```
- Node counts by state
- Total buffer memory
- Wall clock time

**Key bindings:**
| Key | Action |
|-----|--------|
| `Page Up` / `Page Down` | Scroll one page |
| `Shift+Up` / `Shift+Down` | Scroll one line |
| `Ctrl+Home` / `Ctrl+End` | Jump to top / bottom |
| `Esc` | Return to live (auto-scroll) mode |

Note: While scrolled back, auto-scroll pauses. New output is buffered but view stays fixed. Press `Esc` or `Ctrl+End` to resume live mode.

## Milestones

### v0.1 - Basic Split-Screen TUI
- [ ] Split screen: scroll area + CLI input
- [ ] Basic line editing with history
- [ ] `node add`, `node list`, `dep`
- [ ] `show` (using dump_colored)
- [ ] `run`, `status`

### v0.2 - I/O & Pipes
- [ ] `cat`, `write`, `tail -f`
- [ ] `pipe`, `buffers` - inspect data flow
- [ ] Async output streaming to scroll area

### v0.3 - Persistence & Scripting
- [ ] `save`, `load` (TOML format)
- [ ] `source` for scripts
- [ ] `export`, `replay` session

### v0.4 - Execution Control
- [ ] `step`, `pause`, `resume`
- [ ] Breakpoints
- [ ] Error injection

### v0.5 - LLM Features
- [ ] Token tracking
- [ ] Model switching
- [ ] Output comparison
