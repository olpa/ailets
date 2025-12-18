# Notification Queue Rust Redesign - Context

## Objective

Reimplement `notification_queue.py` in idiomatic Rust for use in a future operating system core.

## Key Requirements

1. **Idiomatic Rust** - Redesign is acceptable; don't just port Python code
2. **Performance critical** - This is core OS infrastructure, performance matters
3. **Robust against buggy clients** - Clients may be buggy and should not block the core
4. **Thread-safe** - Cross-thread notifications from worker threads to async clients

## Original Python Implementation

- **File**: `notification_queue.py`
- **Purpose**: Thread-safe queue for handle (integer) notifications
- **Key features**:
  - Async `wait_unsafe()` - clients wait for notifications on handles
  - `subscribe()` - clients register callbacks for notifications
  - `notify()` - worker threads trigger notifications
  - Whitelist system - handles must be registered before use
  - Manual lock management to avoid race conditions

## Python Design Issues

1. **`wait_unsafe` API is inherently risky** (lines 190-223)
   - Requires caller to manually acquire lock before calling
   - Method releases lock during execution
   - Cleanup must re-acquire lock
   - Easy to misuse

2. **Callbacks run under lock** (lines 252-255)
   - Slow/buggy callbacks can block the core
   - Exceptions are silently swallowed

3. **Whitelist race condition** (lines 7-50)
   - Check-then-use pattern between lines 10-30
   - Complex lock acquisition protocol required

4. **No resource limits**
   - Unbounded subscriptions/waiters per handle
   - Memory exhaustion risk

## Rust Design Approach

Created initial design in `notification_queue_design.rs`.

### Key Design Decisions

1. **Explicit handle lifecycle** (lines 61-93)
   - Clients explicitly register and unregister handles
   - Eliminates whitelist race condition
   - Clear ownership semantics

2. **Channels instead of callbacks** (lines 130-159)
   - `Subscription` provides channel receiver
   - Core uses `try_send()` - never blocks on slow clients
   - Bounded channels with configurable size

3. **Separate sync/async APIs** (lines 96-114)
   - `notify()` - sync API for worker threads
   - `wait()` - async API for clients
   - Notifications happen outside locks

4. **Resource limits** (lines 11-24)
   - `max_subscribers_per_handle`
   - `max_waiters_per_handle`
   - `callback_timeout` (for future callback support)

5. **Reduced lock contention** (lines 179-187)
   - `parking_lot::RwLock` (faster than std)
   - `FxHashMap` for integer keys
   - Extract data under lock, notify outside lock

6. **Explicit error handling** (lines 161-175)
   - `QueueError` enum with specific error types
   - No silent failures

### API Structure

```rust
// Main queue
NotificationQueue::new(config) -> Self
NotificationQueue::register_handle(debug_hint) -> Handle
NotificationQueue::unregister_handle(handle) -> ()
NotificationQueue::notify(handle, arg) -> Result<usize>

// Waiting (async)
NotificationQueue::wait(handle) -> Result<i32>
NotificationQueue::wait_timeout(handle, timeout) -> Result<i32>

// Subscription (channel-based)
NotificationQueue::subscribe(handle, channel_size, debug_hint) -> Result<Subscription>
Subscription::try_recv() -> Option<i32>
Subscription::recv() -> Option<i32>
Subscription::recv_async() -> Option<i32>
```

## Current Status

### Completed
- ✅ Analyzed Python implementation and identified issues
- ✅ Defined design principles for OS core use
- ✅ Created comprehensive API design sketch in `notification_queue_design.rs`
- ✅ **Implemented working notification queue** in `src/notification_queue.rs`
  - Explicit handle registration/unregistration
  - Channel-based subscriptions
  - Async wait with oneshot channels
  - Resource limits (max waiters/subscribers per handle)
  - Lock contention minimization (extract under lock, notify outside)
- ✅ **Implemented MemPipe in Rust** in `src/mempipe.rs`
  - Writer/Reader with embedded_io traits
  - Broadcast-style: multiple readers from one writer
  - Proper error handling with `IoError` type
  - Auto-close readers when writer closes
- ✅ **Created CLI demo tool** in `src/bin/mempipe_demo.rs`
  - Equivalent to Python `main()` function
  - Demonstrates 1 writer + 3 readers
  - Successfully tested and working
- ✅ Tests included for both notification queue and mempipe

### Not Yet Implemented
- ❌ Remove tokio dependency (too heavy for OS core - use custom wakers)
- ❌ Arena allocators for waiters/subscribers
- ❌ Lock-free fast path for notifications
- ❌ Memory usage limits/caps beyond per-handle limits
- ❌ Advanced metrics and tracing hooks
- ❌ Comprehensive integration tests
- ❌ Benchmarks vs Python version
- ❌ Replace Python implementation in production

## Next Steps

When continuing this work:

1. **Decide on async runtime strategy**
   - Option A: Remove tokio, use custom wakers
   - Option B: Remove tokio, use blocking APIs only
   - Option C: Keep minimal tokio for prototyping

2. **Implement the core**
   - Start with `QueueInner` implementation
   - Focus on notify path performance
   - Add comprehensive tests

3. **Optimize hot paths**
   - Profile notification latency
   - Consider lock-free structures for read-heavy operations
   - Minimize allocations

4. **Add observability**
   - Metrics: notifications/sec, queue depths, errors
   - Debug APIs: dump current waiters/subscribers
   - Integration with OS logging

5. **Integration with existing ailets system**
   - Replace Python notification_queue.py
   - Ensure FFI compatibility if needed
   - Migration strategy

## Open Questions

- **Async runtime**: What's the minimal async runtime for OS core? Custom wakers?
- **Memory model**: Should we use arenas? Fixed-size pools? Dynamic allocation?
- **Handle semantics**: Should handles be reusable or single-use?
- **Priority**: Do we need prioritized notifications?
- **Batching**: Should we batch notifications for efficiency?

## Files

### Python (original)
- `notification_queue.py` - Original Python implementation
- `mempipe.py` - Python MemPipe implementation

### Rust (new)
- `notification_queue_design.rs` - Initial API design sketch
- `src/notification_queue.rs` - **Working implementation**
- `src/mempipe.rs` - **Working MemPipe implementation using embedded_io traits**
- `src/bin/mempipe_demo.rs` - **CLI demo tool (tested and working)**
- `notif_context.md` - This context file

### Testing
Run the demo:
```bash
cd ailetos
echo -e "hello\nworld\n" | cargo run --bin mempipe_demo
```

Expected output: 3 readers (r1, r2, r3) each receive "hello" and "world" in 4-byte chunks.

## References

- Python code race condition explanation: `notification_queue.py:7-50`
- Python `wait_unsafe` implementation: `notification_queue.py:190-223`
- Python notify implementation: `notification_queue.py:224-256`
- Rust design: `notification_queue_design.rs`
