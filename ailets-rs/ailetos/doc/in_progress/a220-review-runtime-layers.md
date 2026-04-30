# SystemRuntime → IO Bridge: Responsibility Extraction

**Date**: 2026-04-29
**Branch**: a220-review-runtime-layers
**Status**: Planning

---

## Background

`SystemRuntime` was reviewed against the claim that it should be renamed to `IO Bridge`. The review confirmed the name change is justified — the dominant architectural pattern is sync-to-async I/O bridging — but identified three responsibilities that do not belong in an IO Bridge.

---

## Foreign Responsibilities

### 1. DAG state in `ActorShutdown` (`system_runtime.rs:691–713`)

The handler mixes I/O cleanup with lifecycle management:

**I/O cleanup** (belongs in IO Bridge):
- `pipe_pool.close_actor_writers(node_handle, exit_code)` — closes pipe writers
- `channels.retain(...)` — drops reader channels from the internal channel table

**Lifecycle management** (foreign to IO Bridge):
- `dag.set_state(node_handle, NodeState::Terminating)` / `Terminated`
- `dag.set_exit_code(node_handle, exit_code)`
- `spawn_notify.notify_one()`

The executor already owns the "start" side of actor lifecycle — it writes `NodeState::Running` at spawn time (`executor.rs:217`). The "end" side naturally belongs there too.

**Extraction blocker**: ordering constraint. The executor waits on `spawn_notify`, then immediately reads DAG state in `is_ready_to_spawn`. DAG state must be written *before* `notify_one()` fires. Solution: SystemRuntime sends a completion event on a second channel (`actor_done_tx: mpsc::Sender<(Handle, i32)>`); the executor's loop selects on it, does the DAG state write, then re-checks readiness. SystemRuntime drops the lifecycle concern entirely.

### 2. DAG read in `materialize_stdin` (`system_runtime.rs:300–323`)

`materialize_stdin` reads the DAG only to build an `OwnedDependencyIterator` for a new `MergeReader`. This is the sole reason SystemRuntime holds `Arc<RwLock<Dag>>` apart from `ActorShutdown`.

**Extraction path**: The executor knows an actor's dependencies at spawn time. It can construct the `MergeReader` there and pass it into `BlockingActorRuntime::new()` alongside `system_tx`. The `MaterializeStdin` `IoRequest` variant disappears. This is the cleanest extraction — no ordering constraints, no new channels needed.

### 3. `spawn_notify` ownership (`system_runtime.rs:259–260`)

`spawn_notify` is created inside SystemRuntime and lent to the executor via `get_spawn_notify()`. The executor owns the "wait" side; SystemRuntime owns "notify." The dependency arrow points the wrong way.

**Extraction path**: Flip ownership — the executor creates `Arc<Notify>` and passes it into `SystemRuntime::new()`. The IO Bridge becomes a *consumer* (fires it when I/O events occur); the executor is the *owner*. After `ActorShutdown` is extracted, the executor also fires it after DAG state updates. No behavioral change, but the dependency direction becomes correct.

---

## 3-Step Plan

### Step 1 — Flip `spawn_notify` ownership (trivial) ✓ DONE

- Executor creates `Arc<Notify>`, passes it to `SystemRuntime::new()`
- Remove `SystemRuntime::get_spawn_notify()`
- No behavioral change; establishes correct dependency direction before the bigger steps

### Step 2 — Extract `materialize_stdin` to the executor (low risk) ✓ DONE

- At spawn time in the executor, build the `MergeReader` from DAG dependencies
- Pass the reader into `BlockingActorRuntime::new()` (or a new constructor parameter)
- Remove `IoRequest::MaterializeStdin` and `SystemRuntime::materialize_stdin`
- After this step, SystemRuntime no longer needs to read the DAG for I/O wiring

### Step 3 — Extract `ActorShutdown` lifecycle work to the executor (high impact) ✓ DONE

Implementation notes:
- `ActorLifecycleEvent` enum with `Terminating` (reply: prior `NodeState`) and `Terminated` (reply: prior `NodeState`)
- Reply channels preserve ordering: `Terminating` reply gates writer close; `Terminated` reply confirms DAG update
- `handle_actor_shutdown` extracted as `async fn` for linear early-exit flow
- `actor_done_task` uses exhaustive match with warn on unexpected states

- Add `actor_done_tx: mpsc::UnboundedSender<(Handle, i32)>` passed into SystemRuntime
- In `ActorShutdown` handler: keep I/O cleanup (`close_actor_writers`, `channels.retain`); send `(node_handle, exit_code)` on `actor_done_tx` instead of writing DAG state
- Spawn a small `actor_done_task` in the executor that receives completions, writes DAG state (`Terminating` → `Terminated`, `set_exit_code`), and fires `notify`
- Remove the `dag` field from SystemRuntime entirely

### Step 4 — Rename `SystemRuntime` → `IoBridge` ✓ DONE

- Rename `system_runtime.rs` → `io_bridge.rs`
- Rename the struct `SystemRuntime` → `IoBridge` and update all references
- Update `pub mod system_runtime` and re-exports in `lib.rs`
