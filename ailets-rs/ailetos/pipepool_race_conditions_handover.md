# PipePool Race Conditions - Analysis Handover

## Context

This document analyzes race conditions in `src/pipe/pool.rs` `get_or_await_reader()` function, specifically around the pattern where state is checked under lock, then the lock is released, and additional operations are performed based on the stale state.

**File:** `src/pipe/pool.rs:209-346`

## The Lock/Unlock/Match Pattern

The critical pattern is:

```rust
let state_check = {
    let inner = self.inner.lock();  // LOCK
    // ... check pool state ...
    StateCheck::Wait(notify) or StateCheck::CheckNode
}; // UNLOCK

match state_check {  // Use stale decision
    StateCheck::Wait(notify) => {
        notify.notified().await;  // Race window before await
    }
    StateCheck::CheckNode => {
        let node_state = self.dag.read();  // Stale DAG state
        // ... make decisions based on stale state ...
        // ... re-acquire lock and modify pool ...
    }
}
```

## Race Condition #1: Missed Notification (DEADLOCK)

### Severity: Critical - Can cause permanent deadlock

### Location
Lines 220-256

### Scenario

```
Timeline:

Thread A (Reader):
  1. Lock pool (line 221)
  2. Find latent in Waiting state
  3. Clone notify Arc (line 239)
  4. Unlock pool (line 250)
  5. ← RACE WINDOW HERE
  6. Call notify.notified() (line 254)
  7. Start awaiting notification

Thread B (Writer via touch_writer):
  1. (happens during A's race window)
  2. Lock pool (line 392)
  3. Remove latent writer (line 395)
  4. Add realized writer (line 399)
  5. Unlock pool (line 403)
  6. Call notify.notify_waiters() (line 407)

Result: Thread A hangs forever
```

### Root Cause

`tokio::sync::Notify` does not store notifications. Calling `notify_waiters()` before `notified()` means the notification is lost. The notification happens during the race window between extracting the notify handle and actually starting to wait on it.

### Current Mitigation

The loop-and-recheck pattern (line 256 `continue`) is documented as handling this case. **However, this only works if the writer was added to the pool.** If the latent was closed (not realized), the reader loops back and sees `LatentState::Closed`, returning None correctly.

**The deadlock occurs when:** Writer is added, latent removed, notification sent, THEN reader starts waiting. Reader loops back, finds the writer, and returns successfully. **So this race might actually be handled correctly by the loop.**

**Action Required:** Verify through testing or proof that the loop-and-recheck always resolves this race.

---

## Race Condition #2: Latent Created After Writer Exists (DEADLOCK + LEAK)

### Severity: Critical - Can cause permanent deadlock and resource leak

### Location
Lines 246-289 (reader) vs. lines 361-411 (touch_writer)

### Scenario

```
Timeline:

Thread A (Reader):
  1. Lock pool (line 221)
  2. Find no writer, no latent → StateCheck::CheckNode
  3. Unlock pool (line 250)
  4. Check DAG → Running/NotStarted (line 261-263)
  5. Create latent object in local variable (line 274-279)
  6. ← RACE WINDOW HERE

Thread B (Writer via touch_writer):
  1. (happens during A's race window)
  2. Lock pool (line 371)
  3. Find no writer (line 372)
  4. Unlock pool (line 375)
  5. Create writer via async call (line 382-388)
  6. Lock pool (line 392)
  7. Check for latent → none exists yet (line 395)
  8. Add writer to pool (line 399)
  9. Unlock pool (line 403)
  10. Don't notify (no latent found, line 406-407)

Thread A continues:
  7. Lock pool (line 282)
  8. Push latent to pool (line 283) ← BOTH writer AND latent now in pool!
  9. Unlock pool (line 284)
  10. Await notify.notified() (line 288) → HANGS FOREVER
```

### Result

1. **Inconsistent state**: Both writer and latent exist in pool for same key
2. **Deadlock**: Reader waits forever (latent will never be notified again)
3. **Resource leak**: Latent is never cleaned up
4. **Silent failure**: Future `touch_writer` calls return existing writer without notifying orphaned latent (line 372-374)

### Root Cause

No recheck after acquiring lock at line 282. The decision to push latent was made based on stale pool state.

### Fix Required

Before line 283, recheck if writer or latent was created during the race window:

```rust
{
    let mut inner = self.inner.lock();

    // Recheck: writer might have been created during race window
    if let Some(writer) = inner.find_writer(key) {
        let shared_data = writer.share_with_reader();
        let reader_handle = Handle::new(id_gen.get_next());
        return Some(Reader::new(reader_handle, shared_data));
    }

    // Recheck: another thread might have created latent
    if inner.find_latent_writer(key).is_some() {
        // Don't create duplicate, loop back and wait on existing latent
        drop(inner);
        continue;
    }

    inner.latent_writers.push(latent);
}
```

---

## Race Condition #3: DAG State Changes Between Check and Use

### Severity: Medium - Can cause unnecessary waiting

### Location
Lines 258-263 (DAG read) vs. lines 266-290 (use stale state)

### Scenario A: Node Terminates Between Check and Wait

```
Timeline:

Thread A (Reader):
  1. Unlock pool at line 250
  2. Read DAG → sees NodeState::Running (line 261-263)
  3. ← NODE TERMINATES HERE (different thread)
  4. Create latent and wait (lines 274-289)

Result: Reader waits for output that was already produced
```

**Impact**: Reader waits unnecessarily. Eventually the scheduler/system calls `close_actor_writers()` during shutdown, which will notify the latent and return None. Reader doesn't get the data that exists in KV.

### Scenario B: Node Restarts Between Check and Use

```
Timeline:

Thread A (Reader):
  1. Read DAG → Terminated
  2. Start KV lookup (line 291-315)
  3. ← NODE RESTARTS (DAG reset)

Result: Reader gets stale data from previous run instead of waiting for fresh output
```

**Impact**: Depends on whether system semantics allow DAG state to move from Terminated back to NotStarted. If yes, this is a correctness issue.

### Root Cause

DAG state is read without synchronization with pool state. The DAG uses `RwLock` but pool uses separate `Mutex`, so there's no atomicity guarantee.

### Mitigation

The specification (`spec://pipe/pool.md#fulfillable-open`) may define that node state transitions are monotonic and properly ordered. If Terminated → Running transition is impossible, Scenario B doesn't occur.

**Action Required:** Verify node state transition invariants in specification.

---

## Race Condition #4: Multiple Readers Create Duplicate Latents

### Severity: Medium - Resource leak, potential missed notifications

### Location
Lines 246-284

### Scenario

```
Timeline:

Thread A (Reader for key K):
  1. Lock pool, find no writer, no latent → CheckNode
  2. Unlock pool
  3. Check DAG → Running
  4. Create latent object

Thread B (Reader for same key K):
  1. (happens between A's step 3-4)
  2. Lock pool, find no writer, no latent → CheckNode
  3. Unlock pool
  4. Check DAG → Running
  5. Create latent object

Thread A:
  5. Lock pool, push latent
  6. Unlock pool

Thread B:
  6. Lock pool, push latent ← DUPLICATE!
  7. Unlock pool

Both threads await their respective notify handles.

Thread C (Writer via touch_writer):
  1. Lock pool
  2. Remove latent (line 395) - removes FIRST match only
  3. Add writer
  4. Unlock
  5. Notify the removed latent's waiters
```

### Result

1. **Multiple latents** for same key exist in pool
2. **First reader** gets notified and wakes up successfully
3. **Second reader** waits forever (its latent was never removed/notified)
4. **Resource leak**: Orphaned latent remains in pool

### Root Cause

`remove_latent_writer()` at line 154 removes first match only. No check prevents duplicate latents from being created.

### Fix Required

Same as Race #2: Recheck before pushing latent at line 282.

---

## Race Condition #5: Orphaned Latent from Terminated Node

### Severity: Low - Edge case, unusual timing

### Location
Lines 291-315 (Terminated branch) vs. lines 266-290 (Running branch)

### Scenario

```
Timeline:

Thread A (Reader):
  1. Lock pool, find nothing → CheckNode
  2. Unlock pool
  3. Read DAG → Terminated (line 261-263)
  4. Start KV lookup (line 297-304)

Thread B (Reader, same key):
  1. (happens between A's step 2-3, reads stale/inconsistent DAG)
  2. Lock pool, find nothing → CheckNode
  3. Unlock pool
  4. Read DAG → Running (stale read or race in DAG RwLock)
  5. Create latent, push it, wait

Thread A:
  5. Return reader from KV or None

Thread B: Waits forever (node is terminated, will never produce new output)
```

### Result

Thread B hangs indefinitely on latent for a node that will never run again.

### Root Cause

Inconsistent DAG reads by concurrent readers. DAG read is not synchronized with pool state.

### Likelihood

Low - requires Thread B to see stale Running state after Thread A sees fresh Terminated state, which is unlikely with `RwLock` fairness.

### Mitigation

System shutdown should close all latent writers via `close_actor_writers()`, eventually unblocking Thread B.

---

## Race Condition #6: No Recheck Before Pushing Latent (ROOT CAUSE)

### Severity: Critical - Enables races #2, #4, #5

### Location
Line 282-283

### Core Issue

After deciding to create a latent (outside the lock), there's **no recheck** before pushing it to the pool. The code should verify:

1. Writer wasn't created in the meantime (Race #2)
2. Another latent wasn't created (Race #4)
3. Node didn't terminate (Race #5)

This is the **root cause** of multiple race conditions.

### Fix Required

Add recheck logic at line 282:

```rust
{
    let mut inner = self.inner.lock();

    // RECHECK: Pool state might have changed during race window

    if let Some(writer) = inner.find_writer(key) {
        // Writer was created during race window
        drop(inner);
        continue;  // Loop back to return reader
    }

    if let Some(existing_latent) = inner.find_latent_writer(key) {
        // Another reader created latent during race window
        // Don't create duplicate, wait on existing one
        let notify = Arc::clone(&existing_latent.notify);
        drop(inner);
        notify.notified().await;
        continue;
    }

    // Safe to push latent now
    inner.latent_writers.push(latent);
}
```

**Same fix needed at line 332** (None branch for non-existent nodes).

---

## Summary by Severity

### Critical (Deadlock)
- **Race #2**: Latent created after writer exists - permanent deadlock + leak
- **Race #6**: Root cause - no recheck before pushing latent

### Medium (Degraded Service)
- **Race #3**: DAG state changes between check and use - unnecessary waiting
- **Race #4**: Duplicate latents - resource leak, potential missed notifications

### Low / Mitigated
- **Race #1**: Missed notification - likely handled by loop-and-recheck pattern (needs verification)
- **Race #5**: Orphaned latent from terminated node - edge case, shutdown will unblock

---

## Recommended Fixes

### Priority 1: Fix Race #2 and #4 (Critical Deadlock)

Add recheck before pushing latent at lines 282-283 and 332-337:

**File:** `src/pipe/pool.rs`

**Locations:**
- Line 282: Before `inner.latent_writers.push(latent)` in Running/NotStarted branch
- Line 335: Before `inner.latent_writers.push(latent)` in None branch

**Code:**
```rust
{
    let mut inner = self.inner.lock();

    // Recheck: writer might have been created during race window
    if let Some(writer) = inner.find_writer(key) {
        let shared_data = writer.share_with_reader();
        let reader_handle = Handle::new(id_gen.get_next());
        return Some(Reader::new(reader_handle, shared_data));
    }

    // Recheck: another reader might have created latent
    if let Some(existing_latent) = inner.find_latent_writer(key) {
        match existing_latent.state {
            LatentState::Waiting => {
                let notify = Arc::clone(&existing_latent.notify);
                drop(inner);
                notify.notified().await;
                continue;
            }
            LatentState::Closed => {
                return None;
            }
        }
    }

    inner.latent_writers.push(latent);
}
```

### Priority 2: Verify Race #1 Mitigation

Add test case that explicitly triggers the missed notification scenario to verify loop-and-recheck handles it.

### Priority 3: Document DAG State Invariants (Race #3)

Clarify in specification whether node state transitions from Terminated → Running are possible. If not, Race #3 Scenario B doesn't occur.

---

## Testing Strategy

### Test for Race #2 (Critical)

```rust
#[tokio::test]
async fn test_race_reader_creates_latent_after_writer_exists() {
    // Setup: pool with DAG showing Running node
    // Thread A: Start get_or_await_reader, pause after DAG check
    // Thread B: Call touch_writer, complete writer creation
    // Thread A: Resume, attempt to push latent
    // Verify: No latent in pool, reader gets writer immediately
}
```

### Test for Race #4 (Duplicate Latents)

```rust
#[tokio::test]
async fn test_race_two_readers_create_duplicate_latents() {
    // Setup: pool with DAG showing Running node
    // Thread A & B: Both call get_or_await_reader concurrently
    // Verify: Only one latent created in pool
    // Thread C: Call touch_writer
    // Verify: Both readers get notified and receive reader
}
```

---

## Related Files

- `src/pipe/pool.rs:209-346` - `get_or_await_reader()`
- `src/pipe/pool.rs:361-411` - `touch_writer()`
- `src/pipe/pool.rs:413-448` - `close_actor_writers()`

## Related Specifications

- `spec://pipe/pool.md#fulfillable-open` - Producer node state checking
- `spec://executor.md#immediate-values` - Value node data availability

## Related Handovers

- `pipepool_latent_resolution_handover.md` - Background on latent pipe design
- `scheduler_iterator_handover.md` - DAG state transitions

---

**Document prepared by:** Claude Code
**Date:** 2026-03-26
**Context:** Race condition analysis after implementing allow_latent fix (commit b9597fd)
