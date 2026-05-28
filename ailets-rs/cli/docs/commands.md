# dagsh commands

### run

```
run <node>
run <node> --bg
```

`run <node>` submits the DAG execution to the ailetos runtime and then behaves identically to `join <node>` — it waits for the target node to terminate while streaming its output. Ctrl+C returns the user to the prompt; the node keeps running in ailetos.

`run <node> --bg` submits and returns immediately with no output streaming.

Multiple runs can be in flight simultaneously.

### join / await

```
join <node>
await <node>   # synonym
```

Waits for the node to reach `Terminated` state and streams its output while waiting. Ctrl+C returns to the prompt; the node keeps running. `await` is an alias for `join`.

### follow

```
follow <node>
```

Streams output from the node without waiting for termination. Behaves like `tail -f`. Ctrl+C stops following; the node keeps running.

### wait

```
wait suspended <node>
wait terminated <node>
```

Scripting barrier: blocks until the node reaches the given state, then returns. Does not stream output. This is the primary synchronization primitive for scripts — it allows waiting for stabilization before proceeding. The `wait suspended` variant has no equivalent in `join` or `follow`.

### cat

```
cat <node>
```

Prints all output the node has produced so far. Post-hoc, non-streaming.

### kill

```
kill <node>
kill -N <node>
```

Only works on `dbg` nodes. Marks the node as killed; the node checks this flag when it wakes from suspension and exits with an error instead of producing output. The `-N` flag is accepted for forward compatibility but the value is currently ignored.

### To be documented

- `quit` / `exit` / `q`
- `set`
- `node`
- `dep` / `deps`
- `show`
- `status`
- `source` / `load`
- `reset`
- `suspend`
- `resume`
- `write`
- `close`
