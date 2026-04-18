# On-Demand Actor Spawn — Handover

## Context

Actors are currently all spawned upfront before any execute. The goal
(`spec://executor.md#on-demand-spawn`) is to defer spawning until a node's
inputs are available.

`is_ready_to_spawn` in `environment.rs` is already implemented and tested
(6 tests in `tests/on_demand_spawn.rs`). The spawn loop in `RunHandle::run`
needs to be replaced.

## Target spawn loop (pseudocode)

```
// Scheduler yields all reachable nodes in topological order, unfiltered.
// Filter for NotStarted to build pending.
// If dynamic DAG changes are added later, re-build pending on each wakeup.
pending: OrderedSet<Handle> = scheduler.iter()
    .filter(|n| n.state == NotStarted)
    .collect()

loop:
    for node in pending (in topological order):
        if is_ready_to_spawn(node, dag, pipe_pool, suspension):
            pending.remove(node)
            launch actor task  // sets node state to Running
    if pending.is_empty():
        break
    spawn_notify.notified().await
```

The loop runs **concurrently with actors**. `spawn_notify` is an
`Arc<tokio::sync::Notify>` fired by `SystemRuntime` when a pipe is realized
(`handle_open_write` → `touch_writer`) or an actor terminates (`ActorShutdown`
→ `Terminated`).

## Changes required

### 1. Lift PipePool

`PipePool` is created inside `SystemRuntime::new()` and never shared.
Create `Arc<PipePool<K>>` in `RunHandle::run()` and pass it into
`SystemRuntime::new()`.

### 2. Add spawn_notify to SystemRuntime

Add `Arc<tokio::sync::Notify>` to `SystemRuntime`. Fire it in
`handle_open_write` (after `touch_writer` succeeds) and in `ActorShutdown`
(after state transitions to `Terminated`). Expose via a getter so
`RunHandle::run()` can clone it before moving `system_runtime` into its task.

### 3. Replace spawn_ready_actor_tasks with the async spawn loop

Remove `spawn_ready_actor_tasks` and replace with the async `pending`-based
loop in `RunHandle::run`. Hold `system_tx` alive for the duration of the loop;
drop it when `pending` is empty so the `SystemRuntime` channel can close.

### 4. Remove filtering from Scheduler

`SchedulerIter` currently skips `Terminated` nodes. Remove that — the iterator
yields all reachable nodes unfiltered. The spawn loop filters for `NotStarted`
when building `pending`.

## Files changed

| File | What changes |
|------|-------------|
| `ailetos/src/environment.rs` | replace `spawn_ready_actor_tasks` with async spawn loop |
| `ailetos/src/system_runtime.rs` | add `spawn_notify`, fire on write/shutdown, accept `Arc<PipePool>` |
| `ailetos/src/scheduler.rs` | remove `Terminated`-skipping filter |
