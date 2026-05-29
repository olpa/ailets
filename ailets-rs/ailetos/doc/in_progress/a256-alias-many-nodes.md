# A256: Allow aliases to resolve to several concrete nodes

GitHub issue #256.

## Problem

`Environment::add_alias` creates exactly one dependency edge, so an alias
always resolves to a single concrete node. The DAG data structure already
supports multiple edges from one alias node, but neither the API nor the CLI
expose this capability.

The affected commands are `run`, `join`, and `follow`, which call
`Environment::resolve` — a function that returns only the first concrete target.

## What already works

- `Dag` deps list is a flat `Vec` — multiple dep edges per alias are already
  valid data.
- `DependencyIterator` / `OwnedDependencyIterator` traverse all concrete nodes
  reachable through aliases (recursive expansion).
- `TopologicalOrderIter` uses `resolve_dependencies` internally, so it already
  handles multi-target aliases; the executor therefore needs no changes.

## Changes required

### 1. `ailetos/src/environment.rs`

**`add_alias`** — accept a slice of targets instead of one:

```rust
// before
pub fn add_alias(&self, alias_name: String, target: Handle) -> Handle

// after
pub fn add_alias(&self, alias_name: String, targets: &[Handle]) -> Handle
```

Add a new helper that returns all resolved concrete nodes:

```rust
pub fn resolve_all(&self, handle: Handle) -> Vec<Handle> {
    let dag = self.dag.read();
    match dag.get_node(handle).map(|n| &n.kind) {
        Some(NodeKind::Alias) => dag.resolve_dependencies(handle).collect(),
        _ => vec![handle],
    }
}
```

Remove `resolve` (single-handle) entirely — `resolve_all` replaces all its
call sites.

### 2. `cli/src/commands.rs` — `cmd_node_inner`, alias branch

Change the CLI `node alias` command to accept one or more targets:

```
node alias <name> <target> [<target>...]
```

Parse all trailing handle arguments and pass them as a slice to `add_alias`.
Print all target IDs in the confirmation message.

### 3. `cli/src/commands.rs` — `cmd_join`

Currently calls `join_handle(handle)` with a raw (possibly alias) handle.

Replace with:

```rust
let targets = self.env.resolve_all(handle);
for target in targets {
    self.join_handle(target)?;
}
```

Sequential waiting is correct: nodes run concurrently in the executor, so by
the time the second `join_handle` call executes the second node will often
already be done or close to it.

### 4. `cli/src/commands.rs` — `cmd_follow`

Currently calls `env.resolve(handle)` (returns first target only) then spawns
one reader task.

Replace with:

```rust
let targets = self.env.resolve_all(handle);
for target in targets {
    if self.is_terminated_without_stdout(target) {
        continue;
    }
    let writer = OutputSinkWriter::new(Arc::clone(&self.notification_sink), color);
    let future = self.env.pipe_pool.reader_future(
        &self.env.idgen,
        (target, StdHandle::Stdout as isize),
        writer,
    );
    self.reader_tasks.spawn_on(future, self.ailetos_async_rt.handle());
}
```

### 5. `cli/src/commands.rs` — `cmd_run`

Three call sites currently use `resolve`; all are replaced with `resolve_all`.

**Executor submission** — pass the alias handle directly; `TopologicalOrderIter`
already walks all concrete nodes reachable from it:

```rust
// remove: let handle = self.env.resolve(handle);
self.executor.submit(handle, stop_conditions)?;
```

**`wait_handle` / join** — replace the single `join_handle(wait_handle)` call
with a loop over all concrete targets:

```rust
let targets = self.env.resolve_all(handle);
for target in targets {
    self.join_handle(target)?;
}
```

The `wait_handle` pre-computation (topo `last()`) is dropped; sequential joining
across concrete targets is equivalent and correct because the executor runs them
concurrently.

**`attach_stdout_for_run`** — the `else` branch currently calls
`attach_one_node(target, bg, color)` on the single resolved handle. Replace with
a loop over `resolve_all(handle)`:

```rust
} else {
    for target in self.env.resolve_all(handle) {
        self.attach_one_node(target, bg, color);
    }
}
```

Stop conditions (`--one-step`, `--stop-before`, `--stop-after`) are applied
inside the executor's `TopologicalOrderIter` and are unaffected. The interaction
of `--one-step` with a multi-branch alias (does it mean one step per branch or
one step total?) is left as a separate concern.

## Files not changed

| File | Reason |
|------|--------|
| `ailetos/src/dag.rs` | `Dag` already stores N deps per node; iterators already handle multi-target aliases |
| `ailetos/src/traversal.rs` | `TopologicalOrderIter` already skips alias nodes |
| `ailetos/src/executor.rs` | Uses `TopologicalOrderIter`; alias entry points already work |

## Test plan

1. Unit test in `cli/tests/shell.rs`: create a multi-target alias, `run` it —
   verify all concrete targets reach `Terminated`.
2. Unit test: `join` on a multi-target alias — verify all targets are awaited.
3. Unit test: `follow` on a multi-target alias — verify output from all
   concrete targets is collected.
4. Existing tests must continue to pass (single-target aliases are a special
   case of the new `&[Handle]` API).
