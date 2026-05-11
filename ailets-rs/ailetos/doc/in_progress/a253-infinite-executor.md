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
