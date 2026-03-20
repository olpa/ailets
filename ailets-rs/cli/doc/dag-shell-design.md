# DAG Shell Design Document

**Working name:** DAG Shell (dagsh)
**Status:** v0.1 implemented

## Goals

### Primary
- **Validation** - Test and validate implementation of corner cases
- **Conference demos** - Interactive tool to demonstrate DAG execution at conferences
- **LLM playground** - Experiment with LLM pipelines interactively

### Secondary
- **Learning tool** - Help newcomers understand the DAG execution model
- **Debugging aid** - Inspect and troubleshoot real pipelines
- **Reproducibility** - Save and replay scenarios for bug reports

## Current Implementation (v0.1)

### Design Decisions
- **Interface:** Simple REPL (TUI planned for future)
- **Syntax:** Shell-like commands
- **Scripting:** Supported via `source`/`load` commands
- **Variables:** `set var = node ...` and `$var` references
- **Foundation:** Built on `ailetos` crate
- **Line editing:** rustyline with history

### Node Types (from ailetos)

#### NodeKind
- **Concrete** - Actual processing node that executes an actor
- **Alias** - Virtual reference to other nodes

#### Node Creation Patterns
- **Value nodes** - Output constant data, immediately `Terminated`
- **Actor nodes** - Execute registered actor functions
- **Alias nodes** - Virtual nodes referencing other nodes

#### NodeState
- `NotStarted` - Initial state (displayed as `⋯ not built`)
- `Running` - Currently executing (displayed as `⚙ running`)
- `Terminating` - Shutting down (displayed as `⏳ terminating`)
- `Terminated` - Completed (displayed as `✓ built`)

### Implemented Commands

```
# Node Management
node add <actor> [--explain=text]    Add actor node
node value <data> [--explain=text]   Add value node (constant data)
node alias <name> <target>           Add alias node
node list                            List all nodes with status
dep <node> <dependency>              Add dependency
deps <node>                          Show direct dependencies

# Variables
set <var> = node ...                 Assign node to variable
$var                                 Reference variable in commands

# Visualization
show [node]                          Tree view (default: whole DAG)

# Execution
run [node]                           Run the DAG (default: last node)

# I/O
cat <node>                           Show output of a node

# Status
status                               Overall DAG status
status <node>                        Node status

# Session
load <file>                          Run script file (alias: source)
reset                                Clear all nodes and start fresh
help                                 Show help
quit                                 Exit
```

### Example Session

```
$ cargo run
dagsh> set val = node value "hello" --explain="greeting"
Added value node 1: "hello" (greeting)
dagsh> set cat1 = node add cat
Added node 2: cat
dagsh> dep $cat1 $val
Added dependency: 2 depends on 1
dagsh> show
cat.2 [⋯ not built]
└── value.1 [✓ built] # greeting
dagsh> run
Running DAG from node 2...
hello
DAG execution completed.
dagsh> status
Nodes: 2 total, 0 not started, 0 running, 2 terminated
```

### Script Format

Scripts are plain text files with one command per line. Empty lines and lines starting with `#` are ignored.

```bash
# Create nodes with variables
set val = node value "hello" --explain="greeting"
set cat1 = node add cat

# Dependencies use $var references
dep $cat1 $val

run
```

## Future Plans

### v0.2 - Split-Screen TUI

```
┌─────────────────────────────────────────────────────────┐
│  [auto-scroll area]                                     │
│  Async output, log messages, command history            │
│                                                         │
│  12:01:02 [node 2] ⚙ running                            │
│  12:01:02 [node 2] output: Hello, world!                │
│  12:01:03 [node 2] ✓ terminated                         │
├─────────────────────────────────────────────────────────┤
│  dagsh> _                                               │
│  [status bar: node count, running, etc.]                │
└─────────────────────────────────────────────────────────┘
```

### v0.3 - I/O & Pipes
- `tail -f <node>` - Follow output
- `write <node> <data>` - Write to node input
- `pipe`, `buffers` - Inspect data flow

### v0.4 - Persistence
- `save`, `load` (TOML format)
- `export`, `replay` session

### v0.5 - Execution Control
- `step`, `pause`, `resume`
- Breakpoints
- Error injection

### v0.6 - LLM Features
- Token tracking
- Model switching
- Output comparison
