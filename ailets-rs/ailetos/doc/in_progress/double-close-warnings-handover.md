# Double-Close Writer Warnings - Handover Documentation

**Date**: 2026-03-15
**Status**: In Progress - Architectural Decision Made, Implementation Pending
**Issue**: `Writer::close()` called on already closed writer warnings for actor stdout pipes

---

## Issue Description

When running the CLI project, warnings appear during shutdown:

```
2026-03-15T11:32:45.597180Z  WARN ailetos::pipe: Writer::close() called on already closed writer: Pipe.Writer(handle=Handle { id: 7 }, <locked>, hint=pipes/actor-2-Stdout)
2026-03-15T11:32:45.624241Z  WARN ailetos::pipe: Writer::close() called on already closed writer: Pipe.Writer(handle=Handle { id: 8 }, <locked>, hint=pipes/actor-1-Stdout)
2026-03-15T11:32:45.653372Z  WARN ailetos::pipe: Writer::close() called on already closed writer: Pipe.Writer(handle=Handle { id: 10 }, <locked>, hint=pipes/actor-3-Stdout)
2026-03-15T11:32:45.685297Z  WARN ailetos::pipe: Writer::close() called on already closed writer: Pipe.Writer(handle=Handle { id: 12 }, <locked>, hint=pipes/actor-4-Stdout)
2026-03-15T11:32:45.703340Z  WARN ailetos::pipe: Writer::close() called on already closed writer: Pipe.Writer(handle=Handle { id: 15 }, <locked>, hint=pipes/actor-5-Stdout)
```

All warnings are for **actor stdout pipes** being closed twice.

---

## Root Cause Analysis

### The Double-Close Sequence

When a value node shuts down, the following sequence occurs:

1. **Actor explicitly closes stdout** (`environment.rs:216`)
   - Value node calls `awriter.close()`
   - This sends `IoRequest::CloseWriter` to SystemRuntime
   - `handle_close_writer()` calls `writer.close()` → **First close**

2. **Actor shuts down** (`environment.rs:233`)
   - Value node calls `runtime.shutdown()`
   - This sends `IoRequest::ActorShutdown` to SystemRuntime
   - `ActorShutdown` handler calls `pipe_pool.close_actor_writers(node_handle)` (`system_runtime.rs:646`)
   - `close_actor_writers()` iterates all writers for the actor and calls `writer.close()` → **Second close** ⚠️

### Why It's Safe But Wrong

The warning is harmless because `Writer::close()` has an idempotent guard:

```rust
// pipe.rs:179
if shared.closed {
    log::warn!("Writer::close() called on already closed writer: {self:?}");
    return;  // Already closed, do nothing
}
```

However, the warning indicates **architectural confusion** about ownership.

---

## Architectural Discussion

### Original Design Intent

Documentation in `stub_actor_runtime.rs:165-187` states:

> **"it is SystemRuntime's responsibility to close pipes, not the actor's"**
>
> **Design Rationale:**
> - **Ownership**: SystemRuntime owns the pipes via PipePool, so it should clean them up
> - **Centralized cleanup**: All pipe closure happens in one place (`PipePool::close_actor_writers`)
> - **Prevents double-close**: Actor doesn't close pipes that SystemRuntime will close

But this design was **incomplete** - it didn't address the EOF signaling requirement for value nodes.

### The EOF Problem

Value nodes need to signal EOF to downstream readers:
- Write data to stdout
- Close stdout to signal "no more data"
- Without close, downstream reader blocks forever

The explicit `awriter.close()` in `environment.rs:216` was attempting to solve this, but violated the ownership model.

### Architectural Decision

After discussion, the following ownership philosophy was established:

> **"Whoever opened a resource should close it. SystemRuntime should close anything it opened (stdout/stdin/stderr). Actors should only close resources they explicitly opened themselves. The shutdown cleanup is defensive - it should expect everything already closed and warn if it finds anything open."**

**Key Principles:**

1. **SystemRuntime pre-opens pipes algorithmically** based on the graph structure
   - When actors spawn, SystemRuntime creates stdout/stdin/stderr for them
   - **Therefore, SystemRuntime should close these pipes**

2. **Actors never close stdout/stdin/stderr** because they didn't open them
   - Actors only close pipes they explicitly opened (e.g., via custom file operations)

3. **EOF signaling happens automatically** when SystemRuntime closes the writer
   - No need for actors to explicitly signal EOF
   - `ActorShutdown` → SystemRuntime closes stdout → downstream reader gets EOF

4. **`close_actor_writers()` is defensive cleanup**
   - Should expect everything already closed
   - If it finds open pipes → warn (indicates a leak)
   - In normal operation, should find everything already closed

---

## Implementation Plan

### Changes Required

#### 1. Remove Explicit Closes from Value Nodes

**File**: `src/environment.rs`
**Lines**: 210-223

**Current code:**
```rust
let result = awriter
    .write_all(&value_data.data)
    .map_err(|e| format!("Failed to write value: {e:?}"))
    .and_then(|()| {
        awriter
            .close()  // ❌ Remove this
            .map_err(|e| format!("Failed to close writer: {e:?}"))
    })
    .and_then(|()| {
        areader
            .close()  // ❌ Remove this
            .map_err(|e| format!("Failed to close reader: {e:?}"))
    });
```

**New code:**
```rust
// Actors never close stdout/stdin - they didn't open them.
// SystemRuntime will close these pipes during ActorShutdown.
let result = awriter
    .write_all(&value_data.data)
    .map_err(|e| format!("Failed to write value: {e:?}"));
```

#### 2. Implement Algorithmic Close in ActorShutdown

**File**: `src/system_runtime.rs`
**Location**: Around line 641-650 (ActorShutdown handler)

**Current behavior:**
```rust
IoRequest::ActorShutdown { node_handle } => {
    debug!(actor = ?node_handle, "ActorShutdown received");
    // Close all writers for this actor
    pipe_pool.close_actor_writers(node_handle);
    // TODO: also close readers?
}
```

**Questions to resolve:**
- Does `close_actor_writers()` already implement the "algorithmic close" by iterating all pipes with matching `actor_handle`?
- Do we need a separate step to close stdout/stdin/stderr specifically?
- Should we also add `close_actor_readers()` for symmetry?

**Investigation needed**: Determine if current `close_actor_writers()` already closes all pre-opened pipes algorithmically, or if we need explicit close of stdout/stdin/stderr first.

#### 3. Review PipePool Cleanup Behavior

**File**: `src/pipepool.rs`
**Lines**: 385-425 (`close_actor_writers()`)

**Current behavior:**
- Iterates all writers for the actor
- Calls `writer.close()` on each one
- Logs debug message

**Questions:**
- Is this already the "algorithmic close" we need?
- Should it check `is_closed()` before calling `close()` to distinguish expected vs unexpected open pipes?
- Should it warn if it finds open pipes (indicating actor leaked something)?

**Possible enhancement:**
```rust
for (h, s, writer) in writers_to_close {
    if writer.is_closed() {
        // Expected: SystemRuntime or actor already closed it properly
        debug!(key = ?(h, s), "writer already closed (expected)");
    } else {
        // Unexpected: something leaked
        warn!(key = ?(h, s), "closing writer that should have been closed already");
        writer.close();
    }
}
```

---

## Key Files Reference

### 1. `src/pipe.rs`
- **Lines 175-188**: `Writer::close()` method with idempotent guard and warning
- **Lines 100**: `Writer::is_closed()` method (check before closing)
- **Lines 230-237**: `Writer::drop()` implementation
- **Lines 321**: `Reader::is_closed()` method

### 2. `src/pipepool.rs`
- **Lines 385-425**: `close_actor_writers()` - iterates and closes all writers for an actor
- **Line 417**: Where `writer.close()` is called during cleanup

### 3. `src/system_runtime.rs`
- **Lines 540-580**: `handle_close_writer()` - handles explicit close requests from actors
- **Lines 641-650**: `ActorShutdown` handler - triggers cleanup
- **Line 646**: Calls `pipe_pool.close_actor_writers(node_handle)`

### 4. `src/environment.rs`
- **Lines 191-236**: `spawn_value_node_task()` - value node implementation
- **Line 216**: ❌ Explicit `awriter.close()` - **needs removal**
- **Line 221**: ❌ Explicit `areader.close()` - **needs removal**
- **Line 233**: Calls `runtime.shutdown()`

### 5. `src/stub_actor_runtime.rs`
- **Lines 165-187**: Documentation of shutdown and pipe cleanup responsibility
- **Lines 189-208**: `shutdown()` implementation

---

## Investigation Needed

Before implementing, clarify:

1. **Does `close_actor_writers()` already implement algorithmic close?**
   - It iterates all writers with matching `actor_handle`
   - This should include stdout/stdin/stderr that SystemRuntime created
   - Is this already the "algorithmic close" or do we need explicit handling?

2. **Should we also have `close_actor_readers()`?**
   - Currently only writers are closed in ActorShutdown
   - Should readers also be closed symmetrically?

3. **What about pipes actors explicitly opened?**
   - If actors can open custom pipes (not just use pre-opened stdout/stdin/stderr)
   - Those should be closed by the actor before shutdown
   - `close_actor_writers()` should warn if it finds them still open

---

## Testing Plan

After implementation:

1. **Run CLI project** - warnings should disappear
2. **Verify EOF signaling** - downstream readers should still receive EOF when actor completes
3. **Test actor pipe leaks** - if actor opens a pipe but doesn't close it, should see warning
4. **Test normal operation** - no warnings during normal shutdown

---

## Next Steps

1. Investigate whether `close_actor_writers()` already implements algorithmic close
2. Remove explicit `close()` calls from `environment.rs:216,221`
3. Add appropriate warning/logging to distinguish expected vs unexpected open pipes
4. Test with CLI project
5. Verify EOF semantics still work correctly
6. Consider adding `close_actor_readers()` for symmetry

---

## Open Questions

- Should `close_actor_writers()` check `is_closed()` and warn only on unexpected open pipes?
- Is there any other code besides value nodes that explicitly closes stdout/stdin/stderr?
- Should the shutdown sequence have explicit "close std handles" step before defensive cleanup?

---

**Current State**: Analysis complete, architectural decision made, ready for implementation.
