# dagsh commands

The shell uses TCL syntax. Most of TCL is supported via the [Molt](https://github.com/wduquette/molt) interpreter — variables (`set`, `$var`), `if`, `while`, `proc`, `catch`, string/list/math commands, etc. DAG shell commands are registered as TCL commands, so node handles can be stored in variables and passed as arguments:

```tcl
set n [node dbg]
dep $n $other
run $n
```

---

## Node Management

### node

```
node <actor> [--explain=text] [--bytes-before-pause=N]
```

Add an actor node. Available actors: `cat`, `dbg`, `shell_input`. Returns the numeric node handle.

`--explain=text` attaches a human-readable label shown in `show` and `nodes` output. `--bytes-before-pause=N` is only meaningful for `dbg` nodes and sets how many bytes to emit before the actor pauses.

### value

```
value <data> [--explain=text]
```

Add a value node (constant data). The data is stored in the KV store and available as the node's stdout. Returns the numeric node handle.

### alias

```
alias <name> <target> [<target> ...]
```

Add an alias node that resolves to one or more target nodes. Aliases are transparent to dependency resolution and DAG traversal. Returns the numeric node handle.

### nodes

```
nodes
```

List all nodes with their current state.

### dag

```
dag exists <name>
dag handle <name>
```

DAG introspection. `exists` returns `1` if a node named `<name>` exists, `0` otherwise. `handle` returns the numeric handle for `<name>`, or an error if not found.

---

## Dependencies

### dep

```
dep <node> <dependency>
```

Declare that `<node>` depends on `<dependency>`. The executor will not start `<node>` until `<dependency>` has terminated.

---

## Visualization

### show

```
show
show <node>
```

Print a tree view of the DAG. Without an argument, shows the whole DAG rooted at each terminal node. With a node argument, shows the subtree rooted at that node.

---

## Execution

### run

```
run [options] [node]
```

Submit the DAG (or a subtree) to the ailetos executor. Without a node argument, targets the single terminal node; if multiple terminal nodes exist you must specify one. Waits for completion and streams output unless `--bg` is given.

| Option | Description |
|--------|-------------|
| `--bg` | Submit and return immediately; output streams asynchronously to the shell. |
| `--one-step` | Execute only the first ready node, then stop. |
| `--stop-before <node>` | Stop execution before `<node>` runs. |
| `--stop-after <node>` | Stop execution after `<node>` completes. |
| `--color <name>` | Colorize streaming output. Accepts a CSS/X11 color name or a 0–255 terminal index. Only meaningful with `--bg`. |

Ctrl+C while waiting returns to the prompt; the run continues in ailetos.

---

## Job Control

### join

```
join <node>
```

Wait for the node (and all nodes it depends on) to reach `Terminated` state, streaming their output while waiting. Ctrl+C returns to the prompt; nodes keep running.

### follow

```
follow <node> [--color <name>]
```

Attach to the node's stdout and stream output without waiting for termination — like `tail -f`. Ctrl+C stops following; the node keeps running. `--color` accepts a CSS/X11 name or 0–255 index.

### kill

```
kill [-N] <node>
```

Only works on `dbg` nodes. Marks the node as killed; the node exits with an error (default exit code 130) instead of producing output when it next wakes. `-N` sets the exit code (currently accepted but the value is ignored).

---

## I/O

### cat

```
cat <node>[:<stream>]
```

Print all output the node has produced so far. Non-streaming. The optional `:<stream>` suffix selects a specific stream: `stdout` (default), `stderr`, or a numeric file descriptor (e.g. `cat mynode:2`).

---

## Status

### status

```
status
status <node>
```

Without an argument, prints an aggregate summary: total, pending, running, suspended, and terminated node counts. With a node argument, shows detailed per-node state including pipe information.

---

## Debug

### suspend

```
suspend <node>
```

Suspend a running actor. The actor pauses at its next suspension point.

### resume

```
resume <node>
```

Resume a suspended actor (works for `dbg` nodes and general actors).

### wait

```
wait suspended <node>
wait terminated <node>
```

Scripting barrier: block until the node reaches the given state, then return. Does not stream output. `wait suspended` has no equivalent in `join` or `follow` and is the primary way to synchronize on a `dbg` pause before inspecting state. Ctrl+C detaches without stopping the node.

---

## Shell Input

### write

```
write <node> [data]
```

Write data to a `shell_input` actor. If `data` is omitted, writes an empty string.

### close

```
close <node>
```

Close a `shell_input` actor, signalling EOF to whatever is reading from it.

---

## Session

### source / load

```
source <file>
load <file>
```

Read and execute a TCL script file. `load` is an alias for `source`.

### help / ?

```
help
?
```

Print a summary of all commands.

### quit / exit / q

```
quit
exit
q
```

Exit the shell. All three forms are equivalent.
