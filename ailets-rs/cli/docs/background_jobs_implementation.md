# Background Jobs Implementation - Handover Document

## Overview

Implement background job support for the dagsh CLI to solve the blocking issue where `run` hangs while waiting for interactive shell input (e.g., shell_input actor waiting for `write` commands).

## Problem Statement

Currently, `run 3` blocks the shell waiting for the DAG to complete. However, when using the `shell_input` actor, the actor is waiting for shell commands (`write`, `close`), creating a deadlock - the shell can't accept commands because it's blocked waiting for the actor, which is waiting for shell commands.

## Solution: Background Jobs with Foreground Interrupt

### User-Facing Behavior

| Command | Behavior |
|---------|----------|
| `run <node>` | Run in foreground (blocks shell), Ctrl+C interrupts |
| `run <node> --bg` | Run in background (shell remains interactive), Ctrl+C does NOT affect it |
| `fg` | Wait for background job to complete |
| `kill` | Terminate the background job |

### Key Design Decisions

1. **Single job only** - Only one job (foreground OR background) at a time
2. **Ctrl+C behavior**:
   - During foreground run: interrupts the run
   - At prompt with background job: does NOTHING to the job (just prints ^C)
   - At prompt without job: prints ^C
3. **No job queue** - Simple `Option<BackgroundJob>` tracking
4. **Clean termination** - Use tokio's `AbortHandle` for cancellation

## Architecture

### Data Structures

```rust
struct BackgroundJob {
    thread: JoinHandle<()>,
    abort_handle: tokio::task::AbortHandle,
}

struct DagShell {
    env: Arc<Environment<MemKV>>,  // CHANGED: was owned, now Arc
    kv: Arc<MemKV>,
    handles: Vec<Handle>,
    vars: HashMap<String, Handle>,
    bg_job: Option<BackgroundJob>,  // NEW
}
```

### Thread Model

**Foreground run:**
- Runs in main thread using `tokio::Runtime::block_on()`
- Uses `tokio::select!` to race between `env.run()` and `tokio::signal::ctrl_c()`

**Background run:**
- Spawns a new thread
- Thread creates its own `tokio::Runtime`
- Wraps `env.run()` in `Abortable` for cancellation
- Main thread stores `JoinHandle` and `AbortHandle`

## Implementation Plan

### Step 1: Refactor DagShell to use Arc<Environment>

**File:** `src/main.rs`

**Change `DagShell` structure:**
```rust
struct DagShell {
    env: Arc<Environment<MemKV>>,  // Changed from Environment<MemKV>
    kv: Arc<MemKV>,
    handles: Vec<Handle>,
    vars: HashMap<String, Handle>,
    bg_job: Option<BackgroundJob>,  // New field
}

struct BackgroundJob {
    thread: std::thread::JoinHandle<()>,
    abort_handle: tokio::task::AbortHandle,
}
```

**Update `DagShell::new()`:**
```rust
fn new() -> Self {
    let kv = Arc::new(MemKV::new());
    let mut env = Environment::new(Arc::clone(&kv));
    env.actor_registry.register("cat", cat::execute);
    env.actor_registry.register("dbg", dbg_actor::execute);
    env.actor_registry.register("shell_input", shell_input_actor::execute);

    Self {
        env: Arc::new(env),  // Wrap in Arc
        kv,
        handles: Vec::new(),
        vars: HashMap::new(),
        bg_job: None,  // Initialize
    }
}
```

**Update all `self.env` usages:**
- Most usages should work as-is due to `Deref`
- `cmd_reset()` needs special attention - create new Arc

### Step 2: Add Dependencies

**File:** `cli/Cargo.toml`

Add if not already present:
```toml
[dependencies]
tokio = { version = "1", features = ["rt", "signal", "macros", "sync"] }
futures = "0.3"
```

### Step 3: Modify `cmd_run()` for Background Support

**File:** `src/main.rs`

**Parse `--bg` flag:**
```rust
fn cmd_run(&mut self, args: &[&str]) -> Result<(), String> {
    let mut one_step = false;
    let mut stop_before: Option<Handle> = None;
    let mut stop_after: Option<Handle> = None;
    let mut target_arg: Option<&str> = None;
    let mut bg_flag = false;  // NEW

    // Parse arguments
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--one-step" => one_step = true,
            "--bg" => bg_flag = true,  // NEW
            "--stop-before" => {
                i += 1;
                let h = args.get(i).ok_or("--stop-before requires a node")?;
                stop_before = Some(self.parse_handle(h)
                    .ok_or_else(|| format!("Invalid handle: {h}"))?);
            }
            "--stop-after" => {
                i += 1;
                let h = args.get(i).ok_or("--stop-after requires a node")?;
                stop_after = Some(self.parse_handle(h)
                    .ok_or_else(|| format!("Invalid handle: {h}"))?);
            }
            arg if !arg.starts_with("--") => {
                target_arg = Some(arg);
            }
            other => return Err(format!("Unknown option: {other}")),
        }
        i += 1;
    }

    // ... existing target resolution code ...

    // Attach stdout
    self.attach_stdout_for_run(handle, one_step, stop_before, stop_after);

    let stop_conditions = StopConditions {
        one_step,
        stop_before,
        stop_after,
    };

    if bg_flag {
        // Background run
        self.run_background(handle, stop_conditions)?;
    } else {
        // Foreground run with Ctrl+C support
        self.run_foreground(handle, stop_conditions)?;
    }

    println!();
    Ok(())
}
```

**Add helper methods:**
```rust
fn run_foreground(&mut self, handle: Handle, stop_conditions: StopConditions) -> Result<(), String> {
    if self.bg_job.is_some() {
        return Err("Background job already running. Use 'fg' or 'kill' first.".to_string());
    }

    let env = Arc::clone(&self.env);
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;

    rt.block_on(async {
        tokio::select! {
            _ = env.run(handle, stop_conditions) => {
                // Normal completion
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n^C - Interrupted");
            }
        }
    });

    Ok(())
}

fn run_background(&mut self, handle: Handle, stop_conditions: StopConditions) -> Result<(), String> {
    if self.bg_job.is_some() {
        return Err("Background job already running. Use 'fg' or 'kill' first.".to_string());
    }

    let env = Arc::clone(&self.env);

    use futures::future::Abortable;
    let (abort_handle, abort_registration) = futures::future::AbortHandle::new_pair();

    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let future = env.run(handle, stop_conditions);
        let result = rt.block_on(Abortable::new(future, abort_registration));

        match result {
            Ok(_) => tracing::info!("Background job completed"),
            Err(_) => tracing::info!("Background job aborted"),
        }
    });

    self.bg_job = Some(BackgroundJob { thread, abort_handle });
    println!("Started background run (use 'fg' to wait, 'kill' to terminate)");

    Ok(())
}
```

### Step 4: Add `fg` Command

**Add to command match in `execute()`:**
```rust
match cmd {
    // ... existing commands ...
    "fg" => self.cmd_fg(rest)?,
    // ...
}
```

**Implement `cmd_fg()`:**
```rust
fn cmd_fg(&mut self, _args: &[&str]) -> Result<(), String> {
    if let Some(job) = self.bg_job.take() {
        println!("Waiting for background job to complete...");
        job.thread.join()
            .map_err(|_| "Background job panicked".to_string())?;
        println!("Job completed");
        Ok(())
    } else {
        Err("No background job running".to_string())
    }
}
```

### Step 5: Add `kill` Command

**Add to command match in `execute()`:**
```rust
match cmd {
    // ... existing commands ...
    "kill" => self.cmd_kill(rest)?,
    // ...
}
```

**Implement `cmd_kill()`:**
```rust
fn cmd_kill(&mut self, _args: &[&str]) -> Result<(), String> {
    if let Some(job) = self.bg_job.take() {
        println!("Killing background job...");
        job.abort_handle.abort();
        job.thread.join().ok();  // Ignore join errors
        println!("Job killed");
        Ok(())
    } else {
        Err("No background job running".to_string())
    }
}
```

### Step 6: Update Help Text

**File:** `src/main.rs` in `cmd_help()`

Update the help text:
```rust
Execution:
  run [node] [options]                Run the DAG (default: last node)
    --one-step                        Execute only the first ready node
    --stop-before <node>              Stop before executing this node
    --stop-after <node>               Stop after executing this node
    --bg                              Run in background

Job Control:
  fg                                  Wait for background job to complete
  kill                                Terminate background job
```

### Step 7: Update `cmd_reset()`

**File:** `src/main.rs`

Handle background job cleanup:
```rust
fn cmd_reset(&mut self) {
    // Kill background job if running
    if let Some(job) = self.bg_job.take() {
        println!("Killing background job...");
        job.abort_handle.abort();
        job.thread.join().ok();
    }

    self.handles.clear();
    self.vars.clear();

    let mut env = Environment::new(Arc::clone(&self.kv));
    env.actor_registry.register("cat", cat::execute);
    env.actor_registry.register("dbg", dbg_actor::execute);
    env.actor_registry.register("shell_input", shell_input_actor::execute);
    self.env = Arc::new(env);

    println!("DAG cleared.");
}
```

### Step 8: Handle Ctrl+C in Main Loop

**File:** `src/main.rs` in `main()`

Already handled by rustyline, but ensure correct behavior:
```rust
loop {
    match rl.readline("dagsh> ") {
        Ok(line) => {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let _ = rl.add_history_entry(line);
            match shell.execute(line) {
                Ok(true) => {}
                Ok(false) => {
                    println!("Goodbye!");
                    break;
                }
                Err(e) => println!("Error: {e}"),
            }
        }
        Err(ReadlineError::Interrupted) => {
            // Ctrl+C at prompt - just print ^C, don't affect background job
            println!("^C");
            continue;
        }
        Err(ReadlineError::Eof) => {
            println!("Goodbye!");
            break;
        }
        Err(err) => {
            println!("Error: {err:?}");
            break;
        }
    }
}
```

### Step 9: Cleanup on Exit

**Add Drop implementation:**
```rust
impl Drop for DagShell {
    fn drop(&mut self) {
        if let Some(job) = self.bg_job.take() {
            job.abort_handle.abort();
            let _ = job.thread.join();
        }
    }
}
```

## Testing Plan

### Test Script: `scripts/test_bg.dagsh`

Create a test script to verify the implementation:

```bash
# Test background job functionality
# This script should be run interactively to test Ctrl+C behavior

# Create shell_input actor
node add shell_input --explain="Interactive input"

# Create dbg actor that depends on shell_input
node add dbg --bytes=2 --explain="Debug actor"

# Add dependency
dep 2 1

# Show the DAG
show

# This is where we test - the script ends here
# Manually test the following scenarios:

# Scenario 1: Foreground run with Ctrl+C
# run 2
# Press Ctrl+C -> should interrupt and return to prompt

# Scenario 2: Background run
# run 2 --bg
# write 1 "hello"
# close 1
# fg
# Should complete successfully

# Scenario 3: Background run with kill
# run 2 --bg
# kill
# Should abort the job

# Scenario 4: Ctrl+C at prompt with background job
# run 2 --bg
# Press Ctrl+C -> should just print ^C, not kill job
# write 1 "test"
# close 1
# fg
# Should complete successfully
```

### Manual Test Cases

1. **Foreground interrupt:**
   - `run 2`
   - Press Ctrl+C
   - Verify: returns to prompt immediately
   - Verify: can run another command

2. **Background basic:**
   - `run 2 --bg`
   - `write 1 "hello"`
   - `close 1`
   - `fg`
   - Verify: completes successfully

3. **Background kill:**
   - `run 2 --bg`
   - `kill`
   - Verify: job terminates
   - Verify: can start new job

4. **Ctrl+C with background:**
   - `run 2 --bg`
   - Press Ctrl+C
   - Verify: prints ^C, job continues
   - `write 1 "test"`
   - `close 1`
   - `fg`
   - Verify: job completes

5. **Double job prevention:**
   - `run 2 --bg`
   - `run 2 --bg`
   - Verify: error message
   - `kill`

6. **Foreground with background running:**
   - `run 2 --bg`
   - `run 2`
   - Verify: error message
   - `fg` or `kill`

## Edge Cases to Handle

1. **Job panics:** Handle in `fg` with error message
2. **Exit with running job:** Cleanup in `Drop`
3. **Reset with running job:** Kill job first
4. **Invalid commands:** Proper error messages for fg/kill with no job

## Code Locations Summary

| File | Changes |
|------|---------|
| `src/main.rs` | Major changes: Arc<Environment>, BackgroundJob struct, cmd_run refactor, new commands fg/kill |
| `cli/Cargo.toml` | Add tokio with signal feature, futures |
| `scripts/test_bg.dagsh` | New test script |

## Estimated Effort

- Refactoring to Arc: 1 hour
- Background job implementation: 2 hours
- Testing and debugging: 2 hours
- **Total: ~5 hours**

## Dependencies and Risks

### Dependencies
- `tokio` with signal support (Linux/Mac only for Ctrl+C handling)
- `futures` for Abortable

### Risks
1. **Platform compatibility:** `tokio::signal::ctrl_c()` may behave differently on Windows
2. **Actor cleanup:** Aborted actors may leave resources in inconsistent state
3. **Environment thread-safety:** Verify Environment is truly safe to share via Arc

### Mitigation
- Test on target platform (Linux confirmed from env info)
- Document that abort is "best effort" cleanup
- Review Environment implementation for thread safety

## Success Criteria

- ✅ `run <node>` can be interrupted with Ctrl+C
- ✅ `run <node> --bg` runs in background
- ✅ Ctrl+C at prompt does NOT kill background job
- ✅ `fg` waits for background job
- ✅ `kill` terminates background job
- ✅ Only one job at a time (enforced)
- ✅ Clean shutdown with background job running

## Questions for Review

1. Should we support job status command? (Decision: No)
2. Should we support multiple concurrent jobs? (Decision: No, single job only)
3. What happens to actor state after abort? (Decision: Best effort, document limitation)

## References

- Original discussion: Background jobs design conversation
- Related code: `src/shell_input_actor.rs`, `src/dbg_actor.rs`
- Tokio signal docs: https://docs.rs/tokio/latest/tokio/signal/
