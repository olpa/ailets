# A267: Clear permanently-blocked nodes from pending on dependency failure

## Context

When a node fails (exits with non-zero), its dependents sitting in `shared_pending`
can never run. Currently the executor retains them in pending indefinitely, which
causes hangs in any caller waiting for those nodes to complete.

The regression test is already in place:
`ailetos/tests/executor.rs` — `test_failed_node_clears_dependents_from_pending`

## Files to change

- `ailetos/src/executor.rs`

## Process

Work in three commits, each reviewed before the next begins:

1. **Commit 1 — `SpawnReadiness` enum + failing test for `is_ready_to_spawn`**
   Write the test first, get it red, show it to the reviewer, then implement,
   get it green, commit.

2. **Commit 2 — `SpawnOutcome` enum + failing test for `spawn_ready_actors`**
   Same red-green cycle: test first, reviewer looks at it, then implement.

3. **Commit 3 — `remove_blocked_from_pending` + green regression test**
   Wire up the cleanup helper, confirm `test_failed_node_clears_dependents_from_pending`
   passes without `#[ignore]`.

Show the reviewer the test before writing any implementation code.
Show the reviewer the implementation before committing.

---

## Step 1 — `is_ready_to_spawn` → `SpawnReadiness` enum

Replace the `bool` return with a new enum (defined just above the function):

```rust
enum SpawnReadiness {
    Ready,
    Waiting,                   // temporarily blocked; may unblock later
    FailedDependency(Handle),  // dep terminated with non-zero exit code
}
```

Map the current decision table:

| current return | new variant |
|---|---|
| `true` | `Ready` |
| `false` — dep NotStarted, or Running/Terminating without output | `Waiting` |
| `false` — dep Terminated, exit_code != 0 | `FailedDependency(dep_handle)` |

Update all call sites of `is_ready_to_spawn` inside `spawn_ready_actors`.

---

## Step 2 — `spawn_ready_actors` → `SpawnOutcome` enum

Replace the `HashSet<Handle>` return with a new enum (defined just above the function):

```rust
enum SpawnOutcome {
    Ok(HashSet<Handle>),
    // remaining pending; all clear
    FailedNodeDependency(HashSet<Handle>, Handle),
    // remaining pending + handle of the first failed dep found
}
```

Inside `spawn_ready_actors`, when `is_ready_to_spawn` returns
`FailedDependency(dep)` for any pending node, **stop immediately** and return
`SpawnOutcome::FailedNodeDependency(remaining, dep)`.
The first failed dep found is enough — the caller cleans up transitively.

If no failed dependency is found, return `SpawnOutcome::Ok(remaining)` as before.

---

## Step 3 — `remove_blocked_from_pending` + wiring

### Helper function

```rust
fn remove_blocked_from_pending(failed_dep: Handle, pending: &mut HashSet<Handle>, dag: &Dag)
```

**Algorithm — set + FIFO queue BFS:**

```
blocked: HashSet<Handle> = {failed_dep}
queue:   VecDeque<Handle> = [failed_dep]

while let Some(current) = queue.pop_front():
    for each node in pending:
        if current appears in dag.resolve_dependencies(node):
            remove node from pending
            if node not in blocked:
                blocked.insert(node)
                queue.push_back(node)
```

Use `dag.resolve_dependencies(node)` (not `get_direct_dependencies`).
`resolve_dependencies` starts from direct deps but transparently expands
through Alias nodes, yielding only Concrete handles — so it correctly handles
the case where a pending node depends on an alias that resolves to the blocked
concrete node.

### Updated call site in `run_spawn_loop_jobs`

```rust
let pending = mem::take(&mut *shared_pending);
match spawn_ready_actors(pending, env, infra, &mut actor_tasks) {
    SpawnOutcome::Ok(remaining) => {
        *shared_pending = remaining;
    }
    SpawnOutcome::FailedNodeDependency(mut remaining, failed_dep) => {
        let dag = env.dag.read();
        remove_blocked_from_pending(failed_dep, &mut remaining, &dag);
        *shared_pending = remaining;
    }
}
```

---

## Acceptance criterion

`cargo test -p ailetos test_failed_node_clears_dependents_from_pending` passes
(the test carries no `#[ignore]` — it must go green).
