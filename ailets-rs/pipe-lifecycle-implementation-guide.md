# Implementation Guide: Race-Free Pipe Lifecycle Management

## Overview

This document specifies how to implement the actor shutdown and pipe registration logic to prevent race conditions between producer termination and consumer pipe opening.

## Core Problem

**Race condition**: A consumer attempts to open a pipe and waits (latently) AFTER the producer has already begun shutdown, resulting in the consumer waiting forever for a pipe that will never be created.

**Solution**: Make the state check and waiter registration atomic, and ensure shutdown cleanup sees all registered waiters.

---

## Actor State Model

### Required States

Actors must have at least these lifecycle states:

- **STARTING** - Actor is initializing
- **RUNNING** - Actor is operational and can create output pipes
- **TERMINATING** - Shutdown initiated, no new pipes can be created
- **TERMINATED** - Actor fully stopped

### State Transition Rules

**Valid transitions**:
```
STARTING → RUNNING → TERMINATING → TERMINATED
STARTING → TERMINATING → TERMINATED (startup failure)
```

**Critical invariant**: Once an actor enters TERMINATING state, it can NEVER return to RUNNING or create new output pipes.

### Memory Visibility Requirements

State changes MUST be visible across threads immediately. Use:
- Java: `volatile` keyword for state field
- C++: `std::atomic` with sequential consistency
- Go: Access state only through channels or with mutex protection
- Python: threading.Lock around state access
- Rust: `Arc<AtomicU8>` or similar

**Why**: Without proper memory barriers, a thread might read stale state and incorrectly register a waiter after termination begins.

---

## Shutdown Sequence (Producer Side)

### Mandatory Order

When an actor shuts down (graceful or crash), the runtime MUST execute these steps in order:

```
1. Set actor.state = TERMINATING
2. Call registry.close_all_pipes(actor_id)
3. Perform other cleanup (close files, network connections, etc.)
4. Set actor.state = TERMINATED
```

**Critical**: Step 1 MUST complete before Step 2. Setting TERMINATING stops new waiters from being created. Only then is it safe to close existing waiters.

### Crash Handling

If an actor crashes, the runtime MUST still execute the shutdown sequence. Use:
- Exception handlers / panic handlers
- Process supervisors
- Signal handlers (SIGTERM, SIGKILL, etc.)

**Why**: Without this, crashed producers leave orphaned latent waiters that wait forever.

---

## Registry Implementation

### Data Structures

The registry maintains:

1. **Active pipes**: Map of pipe_name → pipe_handle
2. **Latent waiters**: Map of pipe_name → list of waiting consumers
3. **Producer states**: Map of actor_id → state (reference to actor's state)
4. **Pipe ownership**: Map of pipe_name → producer_actor_id

### Global Lock

The registry MUST have a single lock (mutex) that protects all operations on these data structures.

**Lock scope**: The lock protects the *metadata* (maps, lists), not the actual pipe I/O operations.

---

## Consumer: open_pipe() Implementation

### Function Signature

```
open_pipe(pipe_name, consumer_id, validation_function, timeout) → pipe_handle or error
```

**Parameters**:
- `pipe_name`: The pipe to open
- `consumer_id`: Identifier of the consuming actor
- `validation_function`: Function that checks if producer can create pipes
- `timeout`: Maximum time to wait

### Implementation Steps

**CRITICAL SECTION (must be atomic)**:

```
1. Acquire registry.lock

2. Call validation_function()
   - This checks if the producer actor is in a state where it can create pipes
   - Implementation: return (producer.state == STARTING || producer.state == RUNNING)

3. If validation returns false:
   - Release registry.lock
   - Return error: "Producer is not accepting new consumers"

4. If validation returns true:
   - Check if pipe already exists in registry.active_pipes
     - If yes: Get handle, release lock, return handle
     - If no: Create waiter object with timeout

5. Register waiter in registry.latent_waiters[pipe_name]

6. Release registry.lock

END CRITICAL SECTION
```

**After lock release**:

```
7. Block waiting for pipe (with timeout)
   - Either pipe becomes available → return handle
   - Or timeout expires → cleanup waiter, return error
   - Or producer fails → cleanup waiter, return error
```

### Validation Function Details

**Purpose**: Answers "Can the producer actor currently create new output pipes?"

**Correct implementation**:
```
validation_function():
    producer_id = registry.pipe_ownership[pipe_name]
    state = get_actor_state(producer_id)
    return (state == STARTING || state == RUNNING)
```

**Why this works**:
- If state is TERMINATING or TERMINATED, validation fails immediately
- If state is STARTING or RUNNING, it's safe to wait because shutdown hasn't begun
- The state check happens INSIDE the critical section, so it's atomic with waiter registration

**Common mistake to avoid**:
```
// WRONG - Don't check pipe existence
validation_function():
    return pipe_name not in registry.active_pipes  // Creates race!
```

---

## Registry: close_all_pipes() Implementation

### Function Signature

```
close_all_pipes(actor_id)
```

Called by the runtime when an actor enters TERMINATING state.

### Implementation Steps

**CRITICAL SECTION**:

```
1. Acquire registry.lock

2. Find all pipes owned by this actor:
   pipes_to_close = []
   for pipe_name, owner in registry.pipe_ownership:
       if owner == actor_id:
           pipes_to_close.append(pipe_name)

3. For each pipe_name in pipes_to_close:
   - Remove from registry.active_pipes (if exists)
   - Extract all waiters from registry.latent_waiters[pipe_name]
   - Add waiters to a local list: waiters_to_notify

4. Release registry.lock

END CRITICAL SECTION
```

**After lock release**:

```
5. For each waiter in waiters_to_notify:
   - Notify waiter that producer is terminating
   - Waiter's blocked thread wakes up with ProducerTerminatedError

6. Close actual pipe handles (OS-level cleanup)
```

### Why Extract Under Lock, Close Outside Lock

**Under lock**: Extract the list of waiters so no new waiters can be added after we've decided what to close.

**Outside lock**: Actual notification and pipe closure might be slow (I/O, network). Holding the lock would block all other registry operations.

**Safety**: New waiters cannot be created because the actor state is TERMINATING, so validation will fail.

---

## Critical Invariants

### Invariant 1: Atomicity of Check-and-Register

The validation check and waiter registration MUST happen atomically under the same lock acquisition. Never release the lock between these operations.

**Why**: Prevents producer from transitioning to TERMINATING between check and registration.

### Invariant 2: State Before Cleanup

Actor state MUST transition to TERMINATING BEFORE close_all_pipes() is called.

**Why**: Ensures no new waiters can be created after cleanup begins.

### Invariant 3: Single Lock for Registry

All registry operations (open_pipe validation, register waiter, close_pipes extraction) MUST use the same lock.

**Why**: Different locks = no mutual exclusion = race conditions return.

### Invariant 4: Validation is State-Only

The validation function MUST only check actor state, not pipe existence or other dynamic conditions.

**Why**: Other checks create additional race windows. State is the single source of truth.

---

## Testing Requirements

### Race Condition Tests

Implement these tests to verify correctness:

**Test 1: Consumer Opens During Shutdown**
```
1. Start producer in RUNNING state
2. In thread A: Begin producer shutdown (set TERMINATING, but pause before close_pipes)
3. In thread B: Immediately call open_pipe()
4. Resume thread A to complete shutdown
5. Verify: Thread B either gets pipe (if registered before TERMINATING) or immediate error
6. Verify: No waiter is left in latent state forever
```

**Test 2: Concurrent Consumers During Shutdown**
```
1. Start producer
2. Launch 100 consumer threads all calling open_pipe() concurrently
3. Launch shutdown thread
4. Verify: All consumers either succeed or fail cleanly
5. Verify: No orphaned waiters remain
```

**Test 3: Producer Crash**
```
1. Start producer
2. Create several latent waiters
3. Kill producer process ungracefully
4. Verify: Runtime catches crash and calls close_all_pipes()
5. Verify: All waiters are notified of failure
```

---

## Common Implementation Mistakes

### ❌ Mistake 1: Releasing Lock Between Validation and Registration

```
// WRONG
lock.acquire()
can_create = validation()
lock.release()  // DON'T DO THIS
if can_create:
    lock.acquire()
    register_waiter()  // Producer might have terminated here!
    lock.release()
```

**Fix**: Keep lock held for entire sequence.

### ❌ Mistake 2: Cleaning Up Before State Transition

```
// WRONG
close_all_pipes(actor_id)  // Cleanup first
actor.state = TERMINATING  // State second
```

**Fix**: State transition must happen first.

### ❌ Mistake 3: Validation Checks Pipe Existence

```
// WRONG
validation():
    return pipe_name not in active_pipes
```

**Fix**: Only check producer state.

### ❌ Mistake 4: Multiple Locks

```
// WRONG
class Registry:
    state_lock = Lock()    // One lock for state
    waiter_lock = Lock()   // Different lock for waiters
```

**Fix**: Use single lock for all registry operations.

### ❌ Mistake 5: Non-Atomic State Reads

```
// WRONG (in languages with weak memory models)
state = actor.state  // Regular variable, no synchronization
```

**Fix**: Use atomic operations or access state only under lock.

---

## Summary Checklist

Before marking implementation complete, verify:

- [ ] Actor has TERMINATING state distinct from TERMINATED
- [ ] Shutdown sequence: TERMINATING → close_pipes() → TERMINATED
- [ ] Runtime calls close_pipes() even for crashes
- [ ] State variable uses proper memory synchronization
- [ ] open_pipe() holds lock during validation AND registration
- [ ] Validation function only checks actor state
- [ ] close_all_pipes() extracts waiters under lock
- [ ] Single shared lock protects all registry operations
- [ ] Race condition tests pass consistently
- [ ] No orphaned waiters possible in any scenario

---

## Questions to Escalate

If during implementation you encounter:

1. **Performance issues from lock contention** → Discuss fine-grained locking strategies
2. **State needs more granularity** → Discuss additional states (e.g., DRAINING)
3. **Producer restart semantics unclear** → Discuss epoch/versioning approach
4. **Registry becomes bottleneck** → Discuss distributed registry architecture

Contact the architect before deviating from this specification.
