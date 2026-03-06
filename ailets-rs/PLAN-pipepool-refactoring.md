# Plan: PipePool Refactoring - Latent Pipes Simplified

## Background

Read the handover document first: `HANDOVER-a229-latent-pipes.md`

The branch `a229-attach-host-stdout` introduced latent pipes and attachments. While the concepts are sound, the implementation became complex:
- `Pipe` struct with `PipeState` enum (Latent/Realized/ClosedWithoutData)
- Reader handles latent state internally via `ensure_realized()`
- Complex state machine in `pipe.rs`

This refactoring simplifies the architecture by moving latent handling to PipePool.

---

## Design Overview

### Core Idea

Delete the `Pipe` class. Store readers and writers directly in PipePool using three vectors:

1. **readers**: `Vec<(Handle, StdHandle, Reader)>`
2. **latent_writers**: `Vec<LatentWriter>`
3. **writers**: `Vec<(Handle, StdHandle, Writer)>`

Move latent handling from Reader to PipePool:
- `read()`, `write()`, `close()` do NOT handle latent pipes
- Latent handling happens in `get_or_create_reader()` and `create_writer()`

### Use Reader and Writer from `master` Branch

The `master` branch has simple Reader/Writer without latent handling:

```rust
// From master:ailetos/src/pipe.rs

pub struct Writer {
    shared: Arc<Mutex<SharedBuffer>>,
    handle: Handle,
    queue: NotificationQueueArc,
    debug_hint: String,
}

pub struct Reader {
    own_handle: Handle,
    buffer: Arc<Mutex<SharedBuffer>>,
    writer_handle: Handle,
    queue: NotificationQueueArc,
    pos: usize,
    own_closed: bool,
    own_errno: i32,
}
```

These can be used **as-is**. The branch version added complexity to Reader that we're removing.

### New PipePool Structure

```rust
pub struct LatentWriter {
    key: (Handle, StdHandle),
    name: String,
    state: LatentState,
    notify: Arc<tokio::sync::Notify>,
}

pub enum LatentState {
    Waiting,
    Closed,
}

pub struct PipePool<K: KVBuffers> {
    readers: Vec<(Handle, StdHandle, Reader)>,
    latent_writers: Vec<LatentWriter>,
    writers: Vec<(Handle, StdHandle, Writer)>,
    notification_queue: NotificationQueueArc,
    kv: Arc<K>,
}
```

### Key Operations

**1. `get_or_create_reader(key, allow_latent) -> Option<Reader>`** (async)

```
if writer exists for key:
    create Reader from writer.share_with_reader()
    add to readers vector
    return Some(reader)

if latent_writer exists for key:
    if state == Closed:
        return None
    else:
        await on notify
        // after notify, writer should exist
        create Reader from writer
        return Some(reader)

if allow_latent:
    create LatentWriter entry
    await on notify
    // after notify, writer exists
    create Reader from writer
    return Some(reader)
else:
    return None
```

**2. `create_writer(key, name, buffer)`**

```
if latent_writer exists for key:
    remove it, keep its notify

create Writer
add to writers vector

if notify exists:
    notify.notify_waiters()
```

**3. `close_writer(key)`**

```
if latent_writer exists for key:
    set state = Closed
    notify.notify_waiters()
    log warning (abnormal case)

if writer exists for key:
    call writer.close()
```

---

## Milestones

### Milestone 1: Migrate the CLI Example

**Goal**: Get `cli/src/main.rs` working with minimal changes.

**Files to modify**:
1. `ailetos/src/pipe.rs` - Revert to master version (delete Pipe struct, PipeState, keep Reader/Writer)
2. `ailetos/src/pipepool.rs` - New implementation with three vectors
3. `ailetos/src/lib.rs` - Update exports if needed

**Files to check for breakage** (may need minimal updates):
- `ailetos/src/system_runtime.rs`
- `ailetos/src/environment.rs`
- `ailetos/src/attachments.rs`

**Steps**:

1. Create a new branch from `a229-attach-host-stdout`:
   ```bash
   git checkout a229-attach-host-stdout
   git checkout -b a229-pipepool-refactor
   ```

2. Revert `pipe.rs` to master version:
   ```bash
   git show master:ailetos/src/pipe.rs > ailetos/src/pipe.rs
   ```
   Then delete the `Pipe` struct (keep only `SharedBuffer`, `Writer`, `Reader`, `ReaderSharedData`, `WaitAction`).

3. Rewrite `pipepool.rs`:
   - Replace `HashMap<(Handle, StdHandle), Pipe>` with three vectors
   - Implement `get_or_create_reader()` as async function
   - Implement `create_writer()` with latent notification
   - Implement `close_writer()` for both latent and realized cases
   - Delete `PipeRef` (no longer needed with direct vector access)
   - Delete `PipeAccess` enum (replaced by `allow_latent: bool` parameter)

4. Update callers to use new API:
   - `system_runtime.rs`: Update pipe operations
   - `attachments.rs`: Use `get_or_create_reader()`
   - `environment.rs`: Update attachment registration if needed

5. Run tests and the CLI example:
   ```bash
   cd ailetos && cargo test
   cd ../cli && RUST_LOG=info cargo run
   ```

**Success criteria**:
- All existing tests pass
- CLI example runs and exits cleanly
- Real-time stdout/stderr streaming works

### Milestone 2: Update Plan Based on Experience

After completing Milestone 1, update this plan:

1. **Document what changed**: List any unexpected complications or API changes.

2. **Refine remaining work**: Based on actual experience, detail what else needs updating.

3. **Add new test cases**: If you discovered edge cases, document them.

4. **Performance notes**: Any observations about the new design's behavior.

### Milestone 3: Cleanup and Documentation

1. Delete unused code (old `PipeState`, etc.)
2. Update inline documentation
3. Update `HANDOVER-a229-latent-pipes.md` or mark it as superseded
4. Consider deleting `A229-latent-pipe-specification.md` if obsolete

---

## Design Decisions

### Why vectors instead of HashMap?

Simplicity. The number of actors is typically small (5-20), so linear search is fine. Vectors make the three-state model (latent/realized/closed) explicit in the data structure.

### Why move latent handling to PipePool?

Reader and Writer become simple, stateless (in terms of latent vs realized). All coordination logic is in one place (PipePool). This matches the original master design where Reader always has a valid buffer.

### Why keep LatentWriter entries after closing?

To prevent new readers from creating a fresh latent entry and waiting forever. The Closed state signals "this writer will never exist."

However, this is an abnormal case (actor closing without writing). Log a warning when it happens.

### What about multiple readers?

Multiple readers per writer are supported. Each reader has its own position. When a writer is created, all waiting readers are notified and can create their Reader instances from `writer.share_with_reader()`.

---

## Files Reference

**Core files to modify**:
- `ailetos/src/pipe.rs` - Simplify (use master version, delete Pipe)
- `ailetos/src/pipepool.rs` - Complete rewrite

**Files that use PipePool** (check for breakage):
- `ailetos/src/system_runtime.rs` - Main coordinator
- `ailetos/src/environment.rs` - Public API
- `ailetos/src/attachments.rs` - Stdout/stderr forwarding

**Test files**:
- Run `cargo test` in `ailetos/` directory
- Check the 87 tests mentioned in handover still pass

**Example**:
- `cli/src/main.rs` - Integration test

---

## Questions to Resolve During Implementation

1. **Lock granularity**: Start with single mutex. If contention is observed, consider separate locks.

2. **Reader handle in vector**: Store `(Handle, StdHandle, Reader)` or just track readers differently?

3. **Buffer allocation timing**: Currently happens in `realize_pipe()`. Keep this pattern or move to `create_writer()`?

4. **Error handling**: What errors can `get_or_create_reader()` return besides "closed without data"?

---

## Getting Started

```bash
# 1. Read the handover
cat HANDOVER-a229-latent-pipes.md

# 2. Check current state
git log --oneline -5
cargo test

# 3. Start refactoring
git checkout -b a229-pipepool-refactor

# 4. Follow Milestone 1 steps above

# 5. Test frequently
cargo test
cd cli && cargo run
```
