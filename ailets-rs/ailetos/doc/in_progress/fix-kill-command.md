# Task: Fix CLI `kill` Command Implementation

## Problem

The current `kill` command in the CLI is implemented incorrectly:

```rust
// cli/src/main.rs:916
job.runtime_handle
    .block_on(job.bridge.cleanup_actor_io(handle, exit_code))
    .map_err(|e| format!("kill failed: {e}"))?;
```

### Issues

1. **Doesn't actually kill the actor** - only cleans up I/O channels, the actor task keeps running
2. **Breaks encapsulation** - requires direct access to `IoBridge`
3. **Incomplete state management** - doesn't update DAG state or send lifecycle events
4. **Requires `run_with_tx`** - the entire `run_with_tx` function exists primarily to support this broken implementation

## Current Flow

```
CLI kill command
  └─> IoBridge::cleanup_actor_io(handle, exit_code)
      ├─> pipe_pool.flush_close_actor_writers(handle, exit_code)
      └─> channel_table.retain(|(h, _, _)| *h != node_handle)
```

**Result**: I/O is cleaned up, but actor task continues running!

## Proper Implementation

To properly kill an actor, the system needs to:

1. **Abort the actor's tokio task**
   - Requires `Executor` to maintain `HashMap<Handle, JoinHandle<()>>`
   - Call `join_handle.abort()` on the target actor

2. **Update DAG state**
   - Set state to `Terminated`
   - Set exit code to the specified value

3. **Send lifecycle events**
   - Send `ActorLifecycleEvent::Terminating`
   - Send `ActorLifecycleEvent::Terminated`
   - This ensures the executor's lifecycle handler updates state and triggers spawn_wakeup

4. **Clean up I/O** (current implementation already does this)
   - Close and flush pipe writers
   - Remove channel table entries

5. **Trigger spawn_wakeup**
   - Wake the spawn loop so dependent actors can react to the termination

## Proposed Solution

### Step 1: Add Actor Task Tracking to Executor

```rust
struct Executor {
    spawn_wakeup: Arc<tokio::sync::Notify>,
    io_bridge: Arc<IoBridge>,
    lifecycle_tx: mpsc::UnboundedSender<ActorLifecycleEvent>,
    lifecycle_handler: tokio::task::JoinHandle<()>,
    attachment_manager: Arc<AttachmentManager>,
    // NEW: track running actor tasks
    actor_tasks: Arc<Mutex<HashMap<Handle, tokio::task::JoinHandle<()>>>>,
}
```

### Step 2: Add Public Kill API

```rust
impl Executor {
    /// Kill a running actor with the specified exit code.
    ///
    /// This will:
    /// - Abort the actor's task
    /// - Update DAG state to Terminated
    /// - Send lifecycle events
    /// - Clean up I/O resources
    /// - Wake the spawn loop
    pub async fn kill_actor(&self, handle: Handle, exit_code: i32) -> Result<(), String> {
        // 1. Abort the task
        if let Some(task) = self.actor_tasks.lock().remove(&handle) {
            task.abort();
        }

        // 2. Clean up I/O
        self.io_bridge.cleanup_actor_io(handle, exit_code).await?;

        // 3. Send lifecycle events (executor will update DAG)
        // This happens automatically when the task is aborted and Drop runs

        Ok(())
    }
}
```

### Step 3: Expose Kill API via Environment or JobQueue

Option A: Add to `Environment`:
```rust
impl Environment {
    pub async fn kill_actor(&self, handle: Handle, exit_code: i32) -> Result<(), String> {
        // Need access to Executor... problematic
    }
}
```

Option B: Return an `ExecutorHandle` from `run_jobs`:
```rust
pub struct ExecutorHandle {
    // Internal reference to executor infrastructure
}

impl ExecutorHandle {
    pub async fn kill_actor(&self, handle: Handle, exit_code: i32) -> Result<(), String> {
        // Implementation
    }
}

pub async fn run_jobs(...) -> ExecutorHandle {
    // ...
}
```

### Step 4: Update CLI to Use New API

```rust
// cli/src/main.rs
fn cmd_kill(&mut self, args: &[&str]) -> Result<(), String> {
    // Parse handle and exit_code...

    let job = self.bg_job.as_ref().ok_or("No background job running")?;

    job.runtime_handle
        .block_on(job.executor_handle.kill_actor(handle, exit_code))
        .map_err(|e| format!("kill failed: {e}"))?;

    println!("Killed node {} with exit code {}", handle.id(), exit_code);
    Ok(())
}
```

### Step 5: Remove `run_with_tx`

Once the kill command no longer needs direct `IoBridge` access, `run_with_tx` can be removed:
- `run()` becomes the simple one-shot executor
- `run_jobs()` returns `ExecutorHandle` for control

## Complexity Assessment

**Medium complexity** - requires:
- Refactoring executor infrastructure (~100 lines)
- Designing proper public API surface
- Testing actor abort behavior
- Ensuring lifecycle events fire correctly on abort
- Updating CLI integration

**Estimated effort**: 3-4 hours

## Alternative: Document Current Limitation

If proper implementation is deferred, at minimum:
1. Add a comment in CLI explaining the limitation
2. Document that `kill` only cleans I/O, doesn't stop execution
3. Consider renaming to `close_io` or similar to be honest about what it does

## References

- Current implementation: `cli/src/main.rs:913-920`
- `IoBridge::cleanup_actor_io`: `ailetos/src/actor_syscall/io_bridge.rs:433`
- `run_with_tx`: `ailetos/src/executor.rs:476`
