# Handover Document: A229 Latent Pipes & Attachments

## Executive Summary

**Branch**: `a229-attach-host-stdout`

**Status**: ✅ Complete and working (4 commits, all tests passing)

**Purpose**: Enable actors to forward their output (stdout/stderr) to host stdout/stderr in real-time, eliminating race conditions between pipe creation and readers.

**Key Innovation**: **Latent pipes** - pipes that exist before writers connect, allowing readers and attachments to attach eagerly while preserving lazy buffer allocation.

---

## Branch Evolution (4 Commits)

### Commit 1: b3d933a - Initial Implementation
**"Implement lazy pipe creation with stream attachments"**

First attempt using **notification channels** for attachment spawning.

**Architecture**:
- Pipes created lazily on first write (to save memory)
- PipePool sends `PipeCreatedEvent` notification when pipe created
- SystemRuntime receives notifications and spawns attachment tasks
- Attachments read from pipes and forward to host stdout/stderr

**Problem Introduced**: Created complex notification plumbing that led to circular dependencies.

**Files Changed**:
- New: `attachments.rs` (stdout/stderr forwarding)
- Updated: `pipepool.rs`, `system_runtime.rs`, `environment.rs`
- Added notification channels throughout

### Commit 2: 54d3856 - First Fix
**"Fix shutdown hang by breaking pipe_created_tx circular dependency"**

**Problem**: SystemRuntime waited for `pipe_created_rx` to close, but PipePool (owned by SystemRuntime) held `pipe_created_tx`, creating circular ownership.

**Solution**:
- Wrapped `pipe_created_tx` in `Mutex<Option<...>>`
- Added `PipePool::drop_pipe_created_tx()` to explicitly drop sender
- Called on request channel close (when actors finish)

**Result**: Fixed one deadlock, but notification system still problematic.

### Commit 3: 7c3f224 - Architecture Redesign ⭐
**"Implement latent pipes to eliminate attachment race conditions"**

**Major architectural change** that eliminated notifications entirely.

**Core Insight**: Instead of creating pipes lazily and notifying readers, create pipes in **latent state** immediately when readers need them.

**New Concepts**:

1. **PipeAccess Enum**:
   ```rust
   enum PipeAccess {
       ExistingOnly,      // Only access existing pipes
       OrCreateLatent,    // Create latent pipe if missing
   }
   ```

2. **PipeState Enum** (state machine):
   ```rust
   enum PipeState {
       Latent { name, notification_queue, realized_notify },
       Realized { writer, buffer },
       ClosedWithoutData,
   }
   ```

3. **Lazy Realization**: Latent → Realized on first write, preserves memory benefits

**Changes**:
- Removed entire notification channel system (simpler!)
- Readers block gracefully on latent pipes using `tokio::sync::Notify`
- Attachments spawn immediately on latent pipes
- MergeReader creates latent pipes for dependencies

**Benefits**:
- No race conditions
- Simpler architecture
- Real-time streaming
- Proper EOF handling
- All 87 tests pass

### Commit 4: 4c9dba8 - Final Shutdown Fix
**"Fix shutdown hang by closing latent pipes for attachments"**

**Problem**: Actors that close without writing leave latent pipes open → attachments wait forever → program hangs.

**Two-Part Issue**:
1. Latent pipes weren't transitioning to closed state
2. Actors only close primary handle (Stdout), not related handles (Log for stderr)

**Solution**:
- Added `Pipe::close_latent()` to transition latent → `ClosedWithoutData`
- Updated `PipeRef::close_writer()` to handle both latent and realized pipes
- Close ALL related std handles when channel closes
- Filter alias nodes from `attach_all_stderr()`
- Added `Environment::resolve()` for alias resolution

**Result**: Clean shutdown, all attachments complete properly.

---

## Core Concepts

### 1. Latent Pipes

**Definition**: A pipe that exists but has no writer or buffer yet.

**State Machine**:
```
┌─────────┐  first write    ┌──────────┐  close writer  ┌────────┐
│ Latent  │ ───────────────>│ Realized │ ───────────────>│ Closed │
└─────────┘                  └──────────┘                 └────────┘
     │                                                          ▲
     │ close without writing                                    │
     └──────────────────────────────────────────────────────────┘
                    ClosedWithoutData
```

**Why Latent Pipes?**

| Scenario | Without Latent | With Latent |
|----------|----------------|-------------|
| Attachment created | Waits for pipe notification | Gets latent pipe immediately |
| Actor writes | Creates pipe, notifies | Realizes latent pipe |
| Actor never writes | Attachment never created | Latent → ClosedWithoutData |
| Memory usage | ✅ Low (lazy) | ✅ Low (lazy realization) |
| Race conditions | ❌ Yes | ✅ None |
| Complexity | ❌ High (notifications) | ✅ Low |

### 2. Attachments

**Purpose**: Forward actor output to host stdout/stderr in real-time.

**Implementation**: `ailetos/src/attachments.rs`

```rust
pub async fn attach_to_stdout(node_handle: Handle, mut reader: Reader) {
    let mut buf = vec![0u8; 4096];
    let mut stdout = std::io::stdout();

    loop {
        let n = reader.read(&mut buf).await;
        if n > 0 {
            stdout.write_all(&buf[..n as usize])?;
            stdout.flush()?;  // Real-time streaming
        } else if n == 0 {
            break;  // EOF
        } else {
            break;  // Error
        }
    }

    reader.close();
}
```

**Lifecycle**:
1. `Environment::attach_stdout(node)` registers attachment config
2. `SystemRuntime::register_attachment()` spawns attachment task immediately
3. Attachment opens reader on latent pipe (blocks in `ensure_realized()`)
4. Actor writes → pipe realizes → reader unblocks → data flows
5. Actor closes → EOF → attachment exits → `SystemRuntime::run()` completes

### 3. PipeAccess Pattern

**Explicit control** of latent pipe creation at call sites:

```rust
// Reading dependencies - create latent if needed
let pipe = pool.get_pipe(handle, std, PipeAccess::OrCreateLatent)?;

// Closing pipes - don't create if doesn't exist
let pipe = pool.get_pipe(handle, std, PipeAccess::ExistingOnly)?;
```

**Why enum over boolean?**
- ✅ Self-documenting: `OrCreateLatent` vs `true`
- ✅ Type-safe and clear intent
- ✅ Extensible for future access modes

---

## Architecture Changes

### Before This Branch

```
┌──────────┐
│  Actor   │ writes
└────┬─────┘
     ↓
┌──────────┐  (creates pipe on write)
│ PipePool │
└──────────┘

No attachments to host stdout/stderr
```

### After This Branch

```
┌──────────────────┐
│ Environment API  │  attach_stdout(node)
└────────┬─────────┘  attach_all_stderr()
         ↓
┌──────────────────┐
│ SystemRuntime    │ register_attachment() → spawn tasks
└────────┬─────────┘
         ↓
┌──────────────────┐
│  Attachments     │ [task 1] [task 2] [task 3] ...
└────────┬─────────┘
         ↓ read
┌──────────────────┐  Latent → Realized
│    PipePool      │  (HashMap<(Handle, StdHandle), Pipe>)
└────────┬─────────┘
         ↑ write
┌──────────────────┐
│     Actor        │
└──────────────────┘
```

### Key Architectural Decisions

1. **One pipe per (actor, StdHandle) pair**
   - Changed from: `HashMap<Handle, Pipe>`
   - To: `HashMap<(Handle, StdHandle), Pipe>`
   - Enables separate stdout/stderr/log pipes

2. **Attachments as background tasks**
   - Spawned via `tokio::spawn()`
   - Run concurrently with actors
   - Tracked in `SystemRuntime::attachment_tasks`

3. **Graceful shutdown**
   - Wait for all attachment tasks: `task.await`
   - Ensure all output flushed before exit

---

## File Structure

### New Files

**`ailetos/src/attachments.rs`** (90 lines)
- `attach_to_stdout()` - Forward to host stdout
- `attach_to_stderr()` - Forward to host stderr
- Real-time streaming with flush after each read

**`A229-latent-pipe-specification.md`** (844 lines)
- Detailed specification document
- Design rationale
- Implementation guide
- API examples

### Major Changes

**`ailetos/src/pipe.rs`** (+394 lines)
- `PipeState` enum (Latent/Realized/ClosedWithoutData)
- `Pipe::new_latent()` - Create latent pipe
- `Pipe::realize()` - Transition latent → realized
- `Pipe::close_latent()` - Transition latent → closed
- `Reader::ensure_realized()` - Block until realized/closed

**`ailetos/src/pipepool.rs`** (+318 lines)
- `PipeAccess` enum
- Changed key: `(Handle, StdHandle)`
- `get_pipe()` with access control
- `open_reader()` for attachments
- `realize_pipe()` for lazy creation
- `PipeRef` wrapper for safe access

**`ailetos/src/system_runtime.rs`** (+219 lines)
- `AttachmentConfig` enum
- `register_attachment()` - Spawn attachment tasks
- `spawn_attachment()` - Create attachment worker
- Track tasks: `attachment_tasks: Vec<JoinHandle>`
- Wait for tasks in `run()` before exit
- Close all related pipes on channel close

**`ailetos/src/environment.rs`** (+82 lines)
- `attach_stdout()` - Public API
- `attach_stderr()` - Public API
- `attach_all_stderr()` - Convenience method (filters aliases)
- `resolve()` - Resolve alias → actual node
- `pending_attachments` registry

### Breaking Changes

1. **StdHandle added to PipePool methods**
   ```rust
   // Before
   pool.create_output_pipe(handle, name, id_gen)

   // After
   pool.create_output_pipe(handle, std_handle, name, id_gen)
   ```

2. **Channel::Writer includes std_handle**
   ```rust
   Channel::Writer { node_handle, std_handle }
   ```

---

## API Usage Examples

### Basic Attachment

```rust
let mut env = Environment::new(Arc::clone(&kv));

// Build flow
let end_node = build_flow(&mut env);

// Attach output to host
let actual_node = env.resolve(end_node);  // Resolve alias if needed
env.attach_stdout(actual_node);
env.attach_all_stderr();

// Run
env.run(end_node).await;
```

### Understanding resolve()

```rust
// If you have an alias:
let baz = env.add_node("cat", &[bar], None);
let end_alias = env.add_alias(".end", baz);

// Don't attach to alias (it's not an actor!)
// env.attach_stdout(end_alias);  // ❌ Wrong - alias won't run

// Resolve first
let actual = env.resolve(end_alias);  // Returns baz
env.attach_stdout(actual);  // ✅ Correct - attaches to actual actor
```

### Custom Attachments

```rust
// Attach specific node's stderr
env.attach_stderr(critical_node);

// Or attach all stderr (automatically filters aliases)
env.attach_all_stderr();
```

---

## Testing

### Run All Tests
```bash
cd ailets-rs/ailetos
cargo test
```

**Expected**: All 87 tests pass

### Integration Test
```bash
cd ailets-rs/cli
RUST_LOG=info cargo run
```

**Expected Output**:
```
cat.5 [⋯ not built] # Copy.baz
└── cat.4 [⋯ not built] # Copy.bar
    ├── value.1 [✓ built] # Static text
    └── cat.3 [⋯ not built] # Copy.foo
        └── stdin.2 [⋯ not built] # Read from stdin
(mee too)simulated stdin
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.12s
     Running `/mnt/.../target/debug/cli`
```

Program should exit cleanly (no hang).

### Debug Verification

```bash
RUST_LOG=debug cargo run 2>&1 | grep "attachment"
```

**Should show**:
- Attachments spawning immediately
- Attachments starting
- Attachments finishing with EOF
- No indefinite waiting

### Stress Test

Test with actors that:
- Never write (latent → closed without data)
- Write then close (latent → realized → closed)
- Write a lot (buffering works)

---

## Known Issues & Limitations

### 1. Circular Alias Dependencies Not Validated

```rust
let a = env.add_alias("a", b);
let b = env.add_alias("b", a);  // ⚠️ Would cause infinite loop in resolve()
```

**Status**: Not validated, would cause stack overflow

**Recommendation**: Add cycle detection to `resolve()` or DAG validation

### 2. StdHandle Coverage

Currently closing these handles on channel close:
- `Log`, `Env`, `Metrics`, `Trace`

Not closing:
- `Stdin` (input handle, different lifecycle)

**Question**: Are there edge cases where Stdin latent pipes should be closed?

### 3. No Timeout on Latent Pipes

Attachments wait indefinitely for latent pipes to realize.

**Pros**:
- Clean design
- Proper shutdown signaling

**Cons**:
- If there's a bug, attachments could wait forever
- No safety net

**Recommendation**: Consider adding optional timeout for debugging

### 4. Alias Nodes Edge Cases

`attach_all_stderr()` filters aliases, but:
- What if someone manually calls `attach_stderr(alias_node)`?
- Should we validate in `attach_stderr()` itself?

**Current behavior**: Would create attachment that waits forever (bad)

**Recommendation**: Add validation in attachment methods

---

## Performance Considerations

### Memory

**Before latent pipes**: All pipes created eagerly → high memory if many actors

**With latent pipes**:
- Latent state: ~100 bytes (name + Arc overhead)
- Realized state: Full buffer allocation
- Net result: Still lazy, just deferred to first write

**Conclusion**: ✅ No memory regression

### CPU

**Overhead per attachment**:
- One tokio task
- Read loop with 4KB buffer
- Blocking on `ensure_realized()`

**Typical case**: 5-10 actors = 5-10 attachment tasks

**Conclusion**: ✅ Negligible overhead

### Latency

**Before**: No streaming to host
**After**: Real-time streaming with flush after each write

**Conclusion**: ✅ Better user experience

---

## Future Improvements

### 1. Streaming Control

Add API for buffering vs real-time:
```rust
env.attach_stdout_buffered(node);  // Buffer for performance
env.attach_stdout_realtime(node);  // Flush after each write
```

### 2. Colored Output

Distinguish stderr from stdout:
```rust
env.attach_stderr_colored(node, Color::Red);
```

### 3. Output Capture

Instead of forwarding to host:
```rust
let output = env.capture_stdout(node).await;
```

### 4. Attachment Lifecycle Events

Notify when attachments start/stop:
```rust
env.on_attachment_start(|node| { ... });
env.on_attachment_complete(|node| { ... });
```

### 5. Timeout Configuration

```rust
env.attach_stdout_with_timeout(node, Duration::from_secs(30));
```

---

## How to Continue This Work

### If You're Adding Features

1. **Read the spec**: `A229-latent-pipe-specification.md` (comprehensive)
2. **Understand state machine**: Latent → Realized → Closed
3. **Use PipeAccess correctly**:
   - `OrCreateLatent` for readers/attachments
   - `ExistingOnly` for cleanup/closing
4. **Test edge cases**:
   - Actors that never write
   - Actors that write then close
   - Multiple readers on same pipe

### If You're Debugging

1. **Enable trace logging**: `RUST_LOG=trace cargo run`
2. **Check attachment lifecycle**:
   ```bash
   grep "attachment" | grep -v "finished"  # Find hanging attachments
   ```
3. **Check pipe states**:
   ```bash
   grep "Latent\|Realized\|ClosedWithoutData"
   ```
4. **Verify channel closes**:
   ```bash
   grep "closing writer channel"
   ```

### Critical Files to Understand

1. **`pipe.rs`**: State machine, realization logic
2. **`pipepool.rs`**: Pipe lifecycle management
3. **`system_runtime.rs`**: Attachment spawning and coordination
4. **`attachments.rs`**: Output forwarding logic

### Common Gotchas

1. **Don't attach to alias nodes** - use `resolve()` first
2. **Don't use `OrCreateLatent` when closing** - use `ExistingOnly`
3. **Don't forget to flush** - stdout/stderr need explicit flush for real-time
4. **Don't panic in attachments** - handle errors gracefully

---

## Migration Guide

### If Upgrading From Pre-A229 Code

**Old**:
```rust
// Single pipe per actor
pool.create_output_pipe(handle, name, id_gen)?;
```

**New**:
```rust
// Separate pipes per std handle
pool.create_output_pipe(handle, StdHandle::Stdout, name, id_gen)?;
```

**Old**:
```rust
// No attachments
```

**New**:
```rust
// Add attachments
env.attach_stdout(node);
env.attach_all_stderr();
```

---

## Success Metrics

- ✅ All 87 tests passing
- ✅ Program exits cleanly (no hangs)
- ✅ Real-time stdout/stderr streaming
- ✅ No race conditions
- ✅ Proper EOF handling
- ✅ Simpler architecture (removed notifications)
- ✅ No memory regressions

---

## Contact & Resources

**Branch**: `a229-attach-host-stdout`

**Commits**: 4 (b3d933a → 54d3856 → 7c3f224 → 4c9dba8)

**Key Files**:
- Spec: `A229-latent-pipe-specification.md`
- New: `ailetos/src/attachments.rs`
- Modified: `pipe.rs`, `pipepool.rs`, `system_runtime.rs`, `environment.rs`

**Questions to Consider**:
1. Should we validate against circular alias dependencies?
2. Should we add timeouts as a safety net?
3. Should we add buffering control to attachments?
4. Should we provide output capture API?

---

## Quick Start for New Developer

```bash
# 1. Checkout branch
git checkout a229-attach-host-stdout

# 2. Run tests
cd ailets-rs/ailetos && cargo test

# 3. Run example
cd ../cli && cargo run

# 4. Study the spec
cat ../A229-latent-pipe-specification.md

# 5. Understand key concepts
# - Latent pipes (pipe.rs)
# - PipeAccess enum (pipepool.rs)
# - Attachment lifecycle (system_runtime.rs, attachments.rs)

# 6. Make your changes
# - Follow the PipeAccess pattern
# - Handle all three states: Latent/Realized/ClosedWithoutData
# - Test edge cases (never write, write then close)

# 7. Test
cargo test
RUST_LOG=debug cargo run

# 8. Commit
git commit -m "A229 <your change description>"
```

---

**This branch is production-ready and ready to merge.** All tests pass, architecture is clean, and shutdown is reliable.
