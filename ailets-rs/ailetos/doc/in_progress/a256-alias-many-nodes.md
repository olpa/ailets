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

---

## Code review findings

Reviewed branch `a256-alias-many-nodes`. Five confirmed bugs and one plausible
bug were found.

### Bug 1 — `cmd_run --stop-before` with no explicit target hangs (CONFIRMED)

**File:** `cli/src/commands.rs`, line 279

When `--stop-before C` is given with no explicit target, `handle` is set to `C`
(lines 260-262). The executor submits `C` with `stop_before=C`, so C is never
run and never reaches `Terminated`. `join_handles(resolve_all(C))` returns
`[C]` and polls forever.

This is the most reliably reproducible hang — it requires no multi-target alias,
only a bare `run --stop-before <node>`.

### Bug 2 — `join_handles` ignores stop conditions on multi-target aliases (CONFIRMED)

**File:** `cli/src/commands.rs`, line 279

`resolve_all(handle)` returns all concrete targets of an alias. With
`--stop-before`/`--stop-after`, the executor only runs a subset of them. The
removed `wait_handle` logic used `TopologicalOrderIter::with_stop_conditions`
to identify the precise last node the executor would touch. The replacement
`join_handles(resolve_all(handle))` waits for all concrete targets regardless,
blocking indefinitely on nodes the executor deliberately excluded.

Example: `run --stop-after B alias` where alias resolves to `[A, B, C, D]`.
Executor runs A and B then stops; `join_handles` waits on C and D forever.

### Bug 3 — `join_handles` with `--one-step` on a multi-target alias hangs (CONFIRMED)

**File:** `cli/src/commands.rs`, line 279

Same root cause as Bug 2. With `--one-step`, the executor advances exactly one
`NotStarted` node. If `resolve_all(handle)` returns multiple targets,
`join_handles` waits for all of them, blocking on the ones not run this step.

Does not affect single-target aliases (the common current case).

### Bug 4 — `attach_one_node` receives unresolved alias handle via `--stop-after` (CONFIRMED)

**File:** `cli/src/commands.rs`, line 411-412

`stop_after` is set by `parse_handle` with no resolution. `attach_stdout_for_run`
passes it directly to `attach_one_node`, which — after this diff — no longer
calls `env.resolve` internally. If the user passes an alias handle as
`--stop-after`, the reader is attached to the alias node's non-existent stdout
pipe and hangs.

### Bug 5 — `join` on a non-existent handle loops forever (CONFIRMED)

**File:** `cli/src/commands.rs`, line 285-302 (`join_handles` inner loop)

`parse_handle` accepts any valid `i64` without checking DAG membership.
`resolve_all` returns `[handle]` for unknown handles (the `_` arm fires when
`get_node` returns `None`). `join_handles` then polls `get_node(target)` forever,
always getting `None`, which never matches `Some(NodeState::Terminated)`.

`join 9999` (never-created handle) hangs until Ctrl-C.

### Bug 6 — `--stop-before` deps passed to `attach_one_node` may be alias handles (PLAUSIBLE)

**File:** `cli/src/commands.rs`, line 413-420

The `--stop-before` branch collects `get_direct_dependencies(stop_before_handle)`
and passes each to `attach_one_node`. If any dependency is an alias node
(reachable when alias nodes sit upstream of concrete nodes in the DAG),
`attach_one_node` no longer resolves it, attaching a reader to a pipe that is
never written to.

---

## Fix work plan

All confirmed bugs share two root causes:

**Root cause A** — `join_handles(resolve_all(handle))` does not respect stop
conditions. The removed `wait_handle` logic was load-bearing.

**Root cause B** — `attach_one_node` lost its internal `resolve` call; callers
that pass potentially-alias handles (the `--stop-after` path, and the
`--stop-before` deps path) now silently attach readers to alias nodes.

**Root cause C** — `join_handles` has no guard for handles absent from the DAG.

### Fix 1 — Restore stop-condition awareness in `cmd_run` join (fixes Bugs 1, 2, 3)

Replace the naive `join_handles(resolve_all(handle))` with logic that picks the
correct wait target(s) based on stop conditions, mirroring the removed
`wait_handle` block:

```rust
// In cmd_run, after executor.submit(...), before join_handles:
let wait_targets: Vec<Handle> = if one_step {
    // Find the single NotStarted node the executor will actually run.
    let dag = self.env.dag.read();
    TopologicalOrderIter::new(&dag, handle)
        .find(|&n| dag.get_node(n).is_some_and(|nd| nd.state == NodeState::NotStarted))
        .map(|n| vec![n])
        .unwrap_or_default()
} else if stop_before.is_some() || stop_after.is_some() {
    // Find the last node(s) the executor will run given stop conditions.
    let dag = self.env.dag.read();
    TopologicalOrderIter::with_stop_conditions(&dag, handle, stop_conditions.clone())
        .last()
        .map(|n| vec![n])
        .unwrap_or_default()
} else {
    self.env.resolve_all(handle)
};
self.join_handles(wait_targets)?;
```

For multi-target aliases with stop conditions the "last node in topo order" is
still singular — this is a known simplification; a precise multi-branch solution
is deferred.

### Fix 2 — Add `None`-guard to `join_handles` (fixes Bug 5)

In the `join_handles` poll loop, treat a handle absent from the DAG as an error
rather than an infinite wait:

```rust
loop {
    let state = env.dag.read().get_node(target).map(|n| n.state);
    match state {
        Some(NodeState::Terminated) => break,
        None => return Err(format!("Handle {} not found in DAG", target.id())),
        _ => {}
    }
    tokio::time::sleep(POLL_INTERVAL).await;
}
```

Note: `join_handles` currently returns `Result<(), String>` inside a `block_on`
closure; threading the error out requires either propagating via a `Result` in
the `join_all` futures or using a `tokio::sync::oneshot` to signal the outer
scope. The simplest approach: replace `join_all` with a `FuturesUnordered` that
yields `Result<(), String>` and short-circuits on the first error.

### Fix 3 — Resolve alias handles in `attach_stdout_for_run` stop-condition paths (fixes Bugs 4 and 6)

In `attach_stdout_for_run`:

```rust
if let Some(stop_after_handle) = stop_after {
    // resolve in case the user passed an alias handle
    for concrete in self.env.resolve_all(stop_after_handle) {
        self.attach_one_node(concrete, bg, color);
    }
} else if let Some(stop_before_handle) = stop_before {
    let deps: Vec<Handle> = {
        let dag = self.env.dag.read();
        dag.get_direct_dependencies(stop_before_handle).collect()
    };
    for dep in deps {
        // resolve in case dep is an alias node
        for concrete in self.env.resolve_all(dep) {
            self.attach_one_node(concrete, bg, color);
        }
    }
}
```

### Fix 4 — Guard `add_aliases` against empty slice (fixes Bug from `add_aliases(&[])`)

```rust
pub fn add_aliases(&self, alias_name: String, targets: &[Handle]) -> Handle {
    assert!(!targets.is_empty(), "add_aliases requires at least one target");
    // ... rest unchanged
}
```

Or return `Result<Handle, String>` for library use. The CLI already enforces
`!rest.is_empty()`, so this is a defensive API boundary guard.

### Fix order

| Priority | Fix | Bugs addressed |
|----------|-----|----------------|
| 1 | Fix 1: restore stop-condition join logic | 1, 2, 3 |
| 2 | Fix 3: resolve aliases in attach_stdout_for_run | 4, 6 |
| 3 | Fix 2: None-guard in join_handles | 5 |
| 4 | Fix 4: guard add_aliases empty slice | API hygiene |

### Tests to add

- `run --stop-before C` (no explicit target): verify returns without hanging.
- `run --stop-after B alias` (multi-target alias): verify returns after B, C/D
  remain NotStarted.
- `run --one-step alias` (multi-target alias): verify only one node advances.
- `run --stop-after <alias_handle>`: verify stdout is attached to concrete node(s).
- `join <nonexistent_id>`: verify returns an error, not a hang.
