# Background Jobs - Debugging Handover

## Current Status

Background job support has been implemented and partially debugged. The basic infrastructure works but actors aren't executing properly.

## What Works

1. **Background job infrastructure**: ✅
   - `run <node> --bg` creates background job
   - `fg` and `kill` commands work
   - Ctrl+C during foreground run moves job to background
   - Thread synchronization using barriers ensures threads start immediately

2. **Logging infrastructure**: ✅
   - `tracing-subscriber` initialized in main()
   - Default INFO level logging
   - Dbg actor messages changed to info level
   - Logs visible with `RUST_LOG=info`

## Current Problem

**Actors are not executing properly when run in background.**

### Symptoms

```
WARN ailetos::stub_actor_runtime: aread: fd not found actor=Handle { id: 2 } fd=0
WARN ailetos::environment: task error node=Handle { id: 2 } name=dbg error=Failed to read: other error
```

### Analysis

1. When `run 2 --bg` is called, the background thread starts immediately (confirmed by logs)
2. However, actors appear to execute only when `fg` is called, not when the background job starts
3. The dbg actor (node 2) tries to read from stdin (fd 0) but gets "fd not found"
4. This suggests the pipe from shell_input (node 1) to dbg (node 2) is not set up when the actor tries to read

### Timeline from logs

```
run 2 --bg
  -> Background thread starting (immediate)
  -> Started background run (returned to prompt)

write 1 hello
close 1
fg
  -> Waiting for background job...
  -> shell_input actor starting  <-- ACTORS ONLY START HERE
  -> dbg actor starting
  -> aread: fd not found error
```

## Root Cause Hypothesis

The actors are actually being spawned when the background job starts, but:
1. Either they're waiting for some condition before executing
2. Or the pipe setup between dependent nodes happens too late
3. Or there's a race condition in the SystemRuntime/pipe setup

The "fd not found" error specifically indicates that when dbg actor tries to read from stdin (expecting to read from shell_input's stdout), the file descriptor hasn't been registered yet.

## Code Locations

### Background job implementation
- `cli/src/main.rs:469-508` - `run_background()` method
- `cli/src/main.rs:402-467` - `run_foreground()` method
- Uses `std::sync::Barrier` for thread synchronization (line 482-503)

### Logging
- `cli/src/main.rs:818-824` - Tracing subscriber initialization
- `cli/src/dbg_actor.rs:37,47` - Info level logging in dbg actor

### Environment changes
- `ailetos/src/environment.rs:56-250` - Environment with interior mutability
- Uses `Arc<RwLock<AttachmentConfig>>` for thread-safe attachment config

## Next Steps to Debug

### 1. Investigate pipe setup timing

Check when pipes are created between dependent nodes:
- Look at `SystemRuntime` implementation
- Check `PipePool` and how it handles fd registration
- Understand the timing of when stdin/stdout fds become available

### 2. Add more detailed logging

Add debug logs in:
- `ailetos/src/system_runtime.rs` - when pipes are created
- `ailetos/src/pipe.rs` - when fds are registered
- `actor_runtime` - when actors request fd access

### 3. Test simplified scenario

Create minimal test case:
```rust
node add shell_input
node add cat  // instead of dbg
dep 2 1
run 2 --bg
write 1 "test"
close 1
fg
```

See if the same issue occurs with cat actor.

### 4. Check for race conditions

The issue might be that:
- SystemRuntime spawns actor tasks
- Actors immediately try to access their fds
- But PipePool hasn't finished setting up the pipes yet
- Need to ensure pipe setup completes before actors start reading

### 5. Review actor spawning order

In `Environment::spawn_actor_tasks()`:
- Nodes are spawned in topological order (dependencies first)
- But they all start executing concurrently
- shell_input and dbg both start at the same time
- Maybe dbg starts before shell_input has registered its stdout?

## Test Commands

```bash
# Enable logging
RUST_LOG=info cargo run -- --load scripts/test_dbg.dagsh

# Or interactively
RUST_LOG=info cargo run
> node add shell_input
> node add dbg --bytes=2
> dep 2 1
> run 2 --bg
> write 1 hello
> close 1
> fg
```

## Files Modified (Uncommitted)

- `scripts/test_dbg.dagsh` - temporary test changes (can be reverted)

## Recent Commits

- `bb985b2` - Add background job support to dagsh CLI
- `c5b0817` - Add logging infrastructure and thread synchronization

## Questions to Answer

1. Why do actors only execute when `fg` is called, not when the background thread starts?
2. Why is the pipe/fd from shell_input to dbg not available when dbg tries to read?
3. Is there a missing synchronization point between SystemRuntime setup and actor execution?
4. Should actors wait for their input pipes to be ready before attempting to read?

## References

- Original design doc: `docs/background_jobs_implementation.md`
- SystemRuntime: `ailetos/src/system_runtime.rs`
- Pipe handling: `ailetos/src/pipe.rs`
- Actor runtime: `actor_runtime/` crate
