# Task: Refactor Executor API to Unified Start/Submit/Shutdown Pattern

## Context

The current executor has three different public functions (`run`, `run_with_tx`, `run_jobs`) with overlapping functionality and confusing semantics. This branch (`a253-infinite-executor`) was created to support submitting new jobs to a running executor, but the API doesn't reflect this goal clearly.

**Current problems:**
- `run()` - one-shot execution, no job submission
- `run_with_tx()` - exists only to support a broken CLI `kill` command (see `fix-kill-command.md`)
- `run_jobs()` - the right approach, but awkward API (mandatory `events_tx`, no handle returned)
- No unified way to interact with a running executor

**Goal:** Replace all three with a single, clean API based on explicit lifecycle: `start()` → `submit()` → `shutdown()`.

## New API Design

### Core Type

```rust
pub struct Executor {
    // Internal state (job sender, event sender, executor task handle, etc.)
}
```

A single type that represents a running executor. Should be cloneable (backed by `Arc` internally) so it can be shared across threads.

### Public Methods

```rust
impl Executor {
    /// Start a new executor for the given environment.
    ///
    /// # Parameters
    /// - env: The actor environment (DAG, pipe pool, etc.)
    /// - events_tx: Optional channel for receiving ExecutorEvent notifications
    ///
    /// # Returns
    /// An Executor handle for submitting jobs and controlling execution.
    pub fn start(
        env: Arc<Environment>,
        events_tx: Option<mpsc::UnboundedSender<ExecutorEvent>>,
    ) -> Self;

    /// Submit a job (target node) to the executor.
    /// Returns immediately without blocking - the job runs asynchronously.
    ///
    /// # Parameters
    /// - target: The DAG node handle to execute
    /// - stop_conditions: Execution constraints (one_step, stop_before, stop_after)
    ///
    /// # Returns
    /// Ok(()) if queued successfully, Err if executor has shut down
    pub fn submit(
        &self,
        target: Handle,
        stop_conditions: StopConditions,
    ) -> Result<(), SendError<Handle>>;

    /// Kill a running actor with the specified exit code.
    ///
    /// NOTE: This will be implemented as part of fix-kill-command.md task.
    /// For now, you can leave this as a stub or todo!().
    pub async fn kill_actor(&self, handle: Handle, exit_code: i32)
        -> Result<(), String>;

    /// Wait for all submitted jobs to complete.
    /// Does not clean up resources - call cleanup() after this.
    pub async fn wait(&self);

    /// Clean up executor resources (I/O bridge, lifecycle handler, etc.).
    /// Must be called after wait() to ensure proper teardown order.
    pub async fn cleanup(self);

    /// Convenience method: wait for completion then cleanup.
    /// This is what most callers will use.
    pub async fn shutdown(self) {
        self.wait().await;
        self.cleanup().await;
    }
}
```

## Implementation Plan

### Step 1: Design Internal Structure

The `Executor` needs to hold:

1. **Job sender** (`JobSender`) - for `submit()` to queue jobs
2. **Executor task handle** (`tokio::task::JoinHandle`) - the background task running `run_spawn_loop_jobs`
3. **Shared infrastructure** - the current `Executor` struct (spawn_wakeup, io_bridge, lifecycle_tx, etc.)
   - **Decision point:** Should the new public `Executor` wrap the internal one, or replace it?
   - **Recommendation:** Rename internal `Executor` to `ExecutorInfra`, create new public `Executor` that wraps it

Example structure:
```rust
// Internal infrastructure (what's currently called Executor)
struct ExecutorInfra {
    spawn_wakeup: Arc<tokio::sync::Notify>,
    io_bridge: Arc<IoBridge>,
    lifecycle_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
    lifecycle_handler: tokio::task::JoinHandle<()>,
    attachment_manager: Arc<AttachmentManager>,
}

// Public API
pub struct Executor {
    job_sender: JobSender,
    executor_task: Arc<Mutex<Option<tokio::task::JoinHandle<ExecutorInfra>>>>,
    // Or store ExecutorInfra directly if needed for kill_actor
}
```

**Question for you:** Do we need access to `io_bridge` from the public `Executor`?
- If yes (for kill_actor): Store `Arc<ExecutorInfra>` in the public `Executor`
- If no: Just store the task handle and sender

**Answer:** Yes, we'll need it for `kill_actor()` eventually. Store shared infrastructure.

### Step 2: Implement `Executor::start()`

This should:
1. Create the job queue channel (`job_queue()`)
2. Create the `ExecutorInfra` (what's currently in `Executor::new()`)
3. Spawn the executor task running `run_spawn_loop_jobs`
4. Return the public `Executor` with the sender and handles

Pseudo-code:
```rust
pub fn start(
    env: Arc<Environment>,
    events_tx: Option<mpsc::UnboundedSender<ExecutorEvent>>,
) -> Self {
    let (job_sender, job_queue) = job_queue();
    let infra = ExecutorInfra::new(&env, events_tx);

    let env_clone = Arc::clone(&env);
    let executor_task = tokio::spawn(async move {
        let actor_tasks = run_spawn_loop_jobs(
            &env_clone,
            &infra,
            &mut job_queue.rx,
            &StopConditions::default(),
        ).await;
        infra.shutdown(actor_tasks).await;
        infra  // Return infra for potential cleanup
    });

    Self {
        job_sender,
        executor_task: Arc::new(Mutex::new(Some(executor_task))),
        // ... other fields
    }
}
```

**Design question:** Should `StopConditions` be per-job or global?
- Current `run_jobs()` takes global `StopConditions`
- But `submit()` in the new API takes per-job `StopConditions`

**Answer:** The `run_spawn_loop_jobs` currently ignores `stop_conditions` for job queue mode - it only uses them during the topological expansion. So per-job `StopConditions` in `submit()` is correct. You may need to modify `run_spawn_loop_jobs` to handle per-job conditions.

**Decision point:** For now, you can either:
- Keep `stop_conditions` as a parameter to `submit()` but only use it for topological expansion
- Make `stop_conditions` a global setting passed to `start()`
- Defer full per-job support to later

**Recommendation:** Accept `StopConditions` in `submit()` and use it during topological expansion (as currently done). This is simple and matches the current behavior.

### Step 3: Implement `submit()`

This is simple - just forward to the `JobSender`:

```rust
pub fn submit(
    &self,
    target: Handle,
    stop_conditions: StopConditions,
) -> Result<(), SendError<Handle>> {
    // Current JobSender only sends Handle, not StopConditions
    // Decision: Do we need to change JobSender to send (Handle, StopConditions)?
    self.job_sender.submit(target)
}
```

**Design question:** How do we pass `stop_conditions` per-job?

**Options:**
1. Change `JobSender` to send `(Handle, StopConditions)` tuples
2. Store `stop_conditions` globally in `Executor` (passed to `start()`)
3. Ignore per-job `stop_conditions` for now (all jobs use default)

**Recommendation:** Option 3 for initial implementation (simplest). You can add a comment that per-job `StopConditions` is a future enhancement. Most real-world usage will use `StopConditions::default()` anyway.

If you want to implement per-job conditions (Option 1):
- Change `JobSender` and `JobQueue` to use `struct Job { target: Handle, stop_conditions: StopConditions }`
- Update `run_spawn_loop_jobs` to use the per-job conditions during topological expansion

### Step 4: Implement `wait()` and `cleanup()`

**`wait()`:**
- Should block until all jobs complete
- Don't consume `self` - just wait
- Might need to track when executor task finishes

```rust
pub async fn wait(&self) {
    // How do we know when all jobs are done?
    // Option A: Check if executor_task is finished (it finishes when channel closes and work is done)
    // Option B: Add a separate notification mechanism

    // Simplest: Just wait for the executor task to finish
    // But we can't join it without consuming it...

    // Better: Store a oneshot channel that signals when executor is idle?
}
```

**Design challenge:** The executor task doesn't finish until the job channel closes. But we want `wait()` to work without closing the channel (so we can submit more jobs later).

**Solutions:**
1. **Don't support waiting without shutdown** - remove `wait()`, only provide `shutdown()`
2. **Track active jobs** - maintain a counter of in-flight jobs, `wait()` blocks until counter hits zero
3. **Use events** - caller uses the events channel to track completions

**Recommendation:** Start with option 1 (no separate `wait()`), only provide `shutdown()`. This simplifies the implementation significantly. Users who need to wait for specific jobs can use the events channel.

Revised API:
```rust
pub async fn shutdown(self) {
    // 1. Drop job_sender to close the channel
    drop(self.job_sender);

    // 2. Wait for executor task to finish
    if let Some(task) = self.executor_task.lock().await.take() {
        match task.await {
            Ok(infra) => {
                // infra.shutdown() was already called in the task
            }
            Err(e) => {
                warn!("executor task panicked: {}", e);
            }
        }
    }
}
```

### Step 5: Implement `kill_actor()` (stub)

For now, just add a stub:

```rust
pub async fn kill_actor(&self, handle: Handle, exit_code: i32) -> Result<(), String> {
    // TODO: Implement as part of fix-kill-command.md
    // This will require:
    // - Accessing io_bridge from shared infrastructure
    // - Sending lifecycle events
    // - Aborting the actor task
    Err("kill_actor not yet implemented".to_string())
}
```

Or if you want to implement it (this solves the `fix-kill-command.md` task):
- See that task document for full details
- You'll need access to the `ExecutorInfra` (io_bridge, lifecycle_tx, etc.)
- Track actor tasks in a `HashMap<Handle, JoinHandle>`

**Decision:** Defer to separate task or implement now? Your choice!

### Step 6: Update Tests

Update the two `run_jobs` tests in `ailetos/tests/executor.rs`:

**Before:**
```rust
let (ev_tx, _ev_rx) = mpsc::unbounded_channel();
run_jobs(Arc::clone(&env), rx_jobs, StopConditions::default(), ev_tx).await;
```

**After:**
```rust
let executor = Executor::start(Arc::clone(&env), None);
executor.submit(target, StopConditions::default())?;
executor.shutdown().await;
```

For the test that uses events:
```rust
let (ev_tx, mut ev_rx) = mpsc::unbounded_channel();
let executor = Executor::start(Arc::clone(&env), Some(ev_tx));
executor.submit(n1, StopConditions::default())?;
// ... wait for event ...
executor.submit(n2, StopConditions::default())?;
executor.shutdown().await;
```

### Step 7: Update Examples and Environment::run()

The example in `examples/stdin_dag_flow.rs` uses `env.run()`:

**Before:**
```rust
env.run(end_node, StopConditions::default()).await;
```

**After (Option A - keep Environment wrapper):**
```rust
// In environment.rs:
pub async fn run(&self, target: Handle, stop_conditions: StopConditions) {
    let executor = Executor::start(Arc::new(self.clone()), None);
    executor.submit(target, stop_conditions).unwrap();
    executor.shutdown().await;
}
```

**After (Option B - use Executor directly):**
```rust
let executor = Executor::start(Arc::new(env.clone()), None);
executor.submit(end_node, StopConditions::default()).unwrap();
executor.shutdown().await;
```

**Recommendation:** Keep the `Environment::run()` wrapper for convenience (Option A). It's a nice ergonomic helper for simple one-shot execution.

### Step 8: Remove Old Functions

Once tests and examples are updated:
1. Delete `run()` from `executor.rs`
2. Delete `run_with_tx()` from `executor.rs` (mark the CLI as broken temporarily, or update CLI)
3. Delete `run_jobs()` from `executor.rs`
4. Update exports in `lib.rs`

**CLI update:** The CLI currently uses `run_with_tx` to get the `IoBridge`. You have two options:
- **Option A:** Leave CLI broken temporarily with a TODO comment, fix as part of `fix-kill-command.md`
- **Option B:** Expose `io_bridge` from the new `Executor` so CLI can access it (temporary hack)

**Recommendation:** Option A - add a comment in the CLI that it needs updating, commit this refactor without CLI support, then fix CLI in the kill-command task.

## Testing Strategy

1. **Run existing tests** - the two `run_jobs` tests should work with new API
2. **Add new test** - multiple `submit()` calls to verify job queueing works
3. **Test events** - verify events are sent (or not sent if `None`)
4. **Test shutdown** - verify cleanup happens properly

Example new test:
```rust
#[tokio::test]
async fn executor_multiple_submits() {
    let env = Arc::new(Environment::new(Arc::new(MemKV::new())));
    env.actor_registry.write().register("noop", |_| Ok(()));

    let n1 = env.add_node("noop".into(), &[], None);
    let n2 = env.add_node("noop".into(), &[], None);
    let n3 = env.add_node("noop".into(), &[], None);

    let executor = Executor::start(env.clone(), None);
    executor.submit(n1, StopConditions::default()).unwrap();
    executor.submit(n2, StopConditions::default()).unwrap();
    executor.submit(n3, StopConditions::default()).unwrap();
    executor.shutdown().await;

    assert_eq!(env.dag.read().get_node(n1).unwrap().state, NodeState::Terminated);
    assert_eq!(env.dag.read().get_node(n2).unwrap().state, NodeState::Terminated);
    assert_eq!(env.dag.read().get_node(n3).unwrap().state, NodeState::Terminated);
}
```

## Open Questions & Decisions

### 1. Cloneable Executor?

Should `Executor` be `Clone` so it can be shared across threads?

**Use case:** CLI might want to hold a clone in the background job while also using it in command handlers.

**Implementation:** Wrap internal state in `Arc`:
```rust
pub struct Executor {
    inner: Arc<ExecutorInner>,
}

struct ExecutorInner {
    job_sender: JobSender,
    // ... other fields
}
```

**Recommendation:** Yes, make it cloneable. This enables flexible usage patterns.

### 2. StopConditions - Global or Per-Job?

See discussion in Step 3.

**Recommendation:** Accept in `submit()` but ignore for initial implementation (always use default). Add TODO comment for future per-job support.

### 3. Should `wait()` exist separately from `shutdown()`?

See discussion in Step 4.

**Recommendation:** No, only provide `shutdown()`. Users can use events to track individual job completion.

### 4. What happens if you call `submit()` after `shutdown()`?

**Options:**
- Return `Err` (channel is closed)
- Panic
- Silently ignore

**Recommendation:** Return `Err` - that's what `JobSender::submit()` already does when the channel is closed. Document this behavior.

### 5. Re-export or not?

Should we re-export `Executor` from the crate root?

**Current:**
```rust
pub use executor::{run, run_jobs, run_with_tx, ...};
```

**New:**
```rust
pub use executor::Executor;
// Or
pub use executor::{Executor, ExecutorEvent, StopConditions};
```

**Recommendation:** Yes, re-export the main types for convenience.

## Success Criteria

- [ ] All existing tests pass with new API
- [ ] New test for multiple `submit()` calls works
- [ ] Old functions (`run`, `run_with_tx`, `run_jobs`) are deleted
- [ ] CLI is either updated or marked as needing update
- [ ] Documentation (doc comments) is clear and complete
- [ ] Code compiles with no warnings

## Timeline Estimate

- **Step 1-2 (design + start):** 1-2 hours
- **Step 3 (submit):** 30 minutes
- **Step 4 (shutdown):** 1 hour
- **Step 5 (kill stub):** 15 minutes
- **Step 6-7 (update tests/examples):** 1 hour
- **Step 8 (cleanup):** 30 minutes
- **Testing & polish:** 1 hour

**Total: 5-6 hours**

## Questions to Ask Before Starting

1. Should I implement per-job `StopConditions` or defer it?
2. Should I implement `kill_actor()` now or leave it as a stub?
3. Should I update the CLI or leave it broken with a TODO?
4. Any concerns about making `Executor` cloneable?

If you're unsure about any of these, the **safe defaults** are:
- Defer per-job StopConditions
- Leave kill_actor as stub
- Leave CLI with TODO
- Make Executor cloneable

Good luck! This is a nice refactoring that will significantly improve the API clarity.
