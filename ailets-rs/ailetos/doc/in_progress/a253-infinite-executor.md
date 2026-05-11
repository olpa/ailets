# a253: Infinite Executor

## Goal

Support two execution modes:

- **Finite** (current): executor exits once all reachable nodes are done or quiescent.
- **Infinite**: executor blocks indefinitely, waiting for new jobs; exits only when all job senders are dropped.

Both modes must be supported simultaneously via the same underlying executor.

## Current Architecture

`run_with_tx` in `src/executor.rs`:

1. Computes a static `pending: Vec<Handle>` by collecting `TopologicalOrderIter` over the DAG from a single `target` node.
2. Passes `pending` to `run_spawn_loop`, which drains it as actors are spawned.
3. Breaks on quiescence (nothing left to spawn, no active actors) or when `pending` is empty.

The `pending` Vec is a temporary simplification — the real work-discovery mechanism is `TopologicalOrderIter`, which should be invoked per job target rather than once at startup.

## Proposed Design

### Job submission: mpsc queue

New jobs are submitted from multiple parts of the system concurrently, so the job source must be multi-producer. Use an mpsc channel:

```rust
#[derive(Clone)]
pub struct JobSender {
    tx: mpsc::UnboundedSender<Handle>,
}

pub struct JobQueue {
    rx: mpsc::UnboundedReceiver<Handle>,
}

pub fn job_queue() -> (JobSender, JobQueue) { ... }
```

`JobSender` is `Clone + Send + Sync` — distribute clones to any system component that needs to submit work. The executor holds the single `JobQueue`.

### Executor entry point

```rust
pub async fn run(
    env: Arc<Environment>,
    jobs: JobQueue,
    stop_conditions: StopConditions,
)
```

### Inner loop behaviour

For each target arriving from `jobs.rx`, the executor runs `TopologicalOrderIter` on-demand to expand the target's dependency subgraph, merging newly discovered `NotStarted` nodes into its working set.

At quiescence (no pending nodes, no active actors):

- If the channel is closed (all `JobSender` clones dropped) → exit.
- Otherwise → `await` the next job from the channel.

### Usage

**Finite mode** — submit one target, drop the sender, then run:

```rust
let (sender, jobs) = job_queue();
sender.submit(target);
drop(sender);
run(env, jobs, StopConditions::default()).await;
```

**Infinite mode** — distribute sender clones, run concurrently; shutdown by dropping all senders:

```rust
let (sender, jobs) = job_queue();
tokio::spawn(run(Arc::clone(&env), jobs, StopConditions::default()));

sender.submit(target1);
// ... from another component:
sender.clone().submit(target2);
// Shutdown: drop all sender clones.
```

### StopConditions

`one_step`, `stop_before`, `stop_after` remain as-is (they control `TopologicalOrderIter` per job). The former implicit "exit when done" behaviour is now expressed by dropping all `JobSender` clones.

---

## Implementation Plan (TDD)

Each step is one commit: write the failing test first, then the minimal green implementation.

`run_with_tx` / `env.run()` and all existing tests remain untouched throughout. `run_jobs` is a parallel addition.

---

### Step 1 — `JobSender`, `JobQueue`, `job_queue()`

**Red test** (`tests/executor.rs`):
- `job_queue()` compiles and returns `(JobSender, JobQueue)`
- `JobSender::submit` sends a handle
- `JobSender` is `Clone`
- Receiving the handle from `JobQueue` works

**Green**: add the types and constructor to `src/executor.rs`, export from `src/lib.rs`.  
No executor logic changes.

---

### Step 2 — `run_jobs`: finite execution (sender dropped before call)

**Red test**:
```
let (tx, jobs) = job_queue();
tx.submit(target);
drop(tx);              // channel closed immediately
run_jobs(env, jobs, StopConditions::default()).await;
assert target is Terminated
```
Fails: `run_jobs` does not exist.

**Green**: implement `run_jobs` in `src/executor.rs`:

1. Drain the channel upfront with `try_recv` (safe because sender is already dropped).
2. For each drained target, expand via `TopologicalOrderIter` into a `pending: Vec<Handle>`.
3. Set up the same infrastructure as `run_with_tx` (notify, bridge, actor_done channel).
4. Call a new private `run_spawn_loop_jobs` with the pending list.  
   At quiescence `run_spawn_loop_jobs` simply breaks — same behaviour as current `run_spawn_loop`.
5. Same teardown as `run_with_tx`: join actor tasks → `bridge.shutdown()` → drop `actor_done_tx` → join `actor_done_task`.

`run_spawn_loop` is not modified; `run_with_tx` is not touched.

---

### Step 3 — `run_jobs`: infinite mode (executor blocks at quiescence)

**Red test**:
```
let (tx, jobs) = job_queue();
tx.submit(n1);
// tx is NOT dropped

let handle = tokio::spawn(run_jobs(env.clone(), jobs, ...));
// wait for n1 to terminate
assert n1 is Terminated
assert !handle.is_finished()   // executor must still be alive

tx.submit(n2);
drop(tx);
handle.await (with timeout);
assert n2 is Terminated
```
Fails: step 2's implementation exits at quiescence even when the channel is open.

**Green**: change the quiescence branch in `run_spawn_loop_jobs`:

```rust
// was: break
match job_rx.recv().await {
    Some(target) => {
        // expand target via TopologicalOrderIter, extend pending
    }
    None => break,   // all senders dropped — shut down
}
```

No other changes needed: if the channel already contains a buffered job (submitted before quiescence), `recv()` returns it immediately without blocking.

---

### Step 4 — In-flight job pickup (new job starts without waiting for quiescence)

**Red test**:
```
// n1 is a "slow" actor (signals when it starts, then blocks until released)
// n2 is independent of n1

tx.submit(n1);
spawn run_jobs(env, jobs, ...);
wait for n1 to signal it has started   // n1 is Running, executor not quiescent
tx.submit(n2);
release n1

assert n2 is Terminated before n1 finishes
```
Fails: with step 3, n2 is only picked up at quiescence (after n1 finishes), so n2 cannot be Terminated while n1 is still running.

**Green**: add a `try_recv` drain at the **top of every iteration** of `run_spawn_loop_jobs`:

```rust
loop {
    // Pick up any jobs submitted since the last iteration
    while let Ok(target) = job_rx.try_recv() {
        // expand via TopologicalOrderIter, extend pending
    }

    // ... existing spawn logic ...

    if quiescent {
        match job_rx.recv().await { ... }   // from step 3
    } else {
        notify.notified().await;
    }
}
```

New jobs submitted while actors are running are now picked up on the very next loop iteration (triggered by the `notify` that fires when the running actor produces output or terminates), enabling parallelism across independent jobs.

---

### Step 5 — Export public API

**Red test**: `use ailetos::{run_jobs, JobSender, JobQueue, job_queue}` compiles from the crate root.

**Green**: add `pub use` entries in `src/lib.rs`.
