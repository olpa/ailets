# On-Demand Actor Spawn — Handover

## Spec

`spec://executor.md#on-demand-spawn`: actor spawning is deferred until input is
available. This prevents premature resource allocation for nodes that may never
execute.

## What is done

- `is_ready_to_spawn` (ailetos/src/environment.rs) — decision function with
  decision table documented in-code and covered by 6 TDD tests
  (ailetos/tests/on_demand_spawn.rs).
- `spawn_ready_actor_tasks` in `RunHandle` loops until no new nodes are spawned,
  setting `NodeState::Running` before each actor starts.

## What is NOT done yet

`is_ready_to_spawn` is not wired into `spawn_ready_actor_tasks`. The loop runs
the wrong readiness check (dep state only, no pipes, no suspension) and runs
entirely before any actor executes — so all nodes are still spawned upfront.

## Core problem

True on-demand spawn requires the spawn loop to run **concurrently with actors**,
reacting to two events:

1. **Pipe realized** — a dep calls `open_write` → `touch_writer` in
   `SystemRuntime::handle_open_write`. Running dep now has output → unblocks
   dependents.
2. **Actor terminated** — `ActorShutdown` sets `Terminated`. If the dep produced
   no output, it is neutral (skip); if it did, it unblocks dependents. Either
   way, re-evaluation is needed.

## Structural issues to resolve

### 1. PipePool is invisible to RunHandle
`PipePool` is created inside `SystemRuntime::new()` and never shared.
`is_ready_to_spawn` needs it. Fix: create `Arc<PipePool<K>>` in
`RunHandle::run()` and pass it into `SystemRuntime::new()` instead.

### 2. No notification mechanism
Nothing wakes the spawn loop when a pipe is realized or a node terminates.
`SuspensionState` uses `Arc<Notify>` for wakeups — same pattern needed here.
A shared `Arc<Notify>` (call it `spawn_notify`) should be fired by
`SystemRuntime` in `handle_open_write` (after `touch_writer` succeeds) and in
`ActorShutdown` (after state set to Terminated). The spawn loop awaits it
between passes.

### 3. system_tx dropped too early
Currently dropped right after the initial spawn loop. If the loop runs
concurrently with actors it must stay alive until spawning is complete.

### 4. Scheduler is not reactive
The `Scheduler` does a full topological sort and returns all reachable
`NotStarted` nodes. It already skips `Terminated`. For the reactive loop, keep
the scheduler as-is and filter its output through `is_ready_to_spawn`. The loop
terminates when the scheduler yields zero `NotStarted` nodes (all remaining are
Running/Terminated or unreachable).

## Proposed implementation steps

1. **Lift PipePool**: change `SystemRuntime::new()` to accept
   `Arc<PipePool<K>>`. Create it in `RunHandle::run()` before constructing
   `SystemRuntime`.

2. **Add spawn_notify**: add `Arc<Notify>` to `SystemRuntime`. Fire it in
   `handle_open_write` (pipe realized) and `ActorShutdown` (node terminated).
   Expose via a getter so `RunHandle::run()` can clone it before moving
   `system_runtime` into the tokio task.

3. **Make spawn loop async**: replace the synchronous loop with an async loop
   that awaits `spawn_notify.notified()` when no nodes were spawned in a pass.
   Keep `system_tx` alive for the duration.

4. **Wire is_ready_to_spawn**: replace the inline dep-state check in
   `spawn_ready_actor_tasks` with a call to `is_ready_to_spawn`, passing the
   shared `PipePool` and `SuspensionState`.

5. **Loop termination**: stop when `Scheduler` yields no `NotStarted` nodes
   (the scheduler already skips Terminated; if all remaining nodes are Running
   or Terminated, the iterator is empty).
