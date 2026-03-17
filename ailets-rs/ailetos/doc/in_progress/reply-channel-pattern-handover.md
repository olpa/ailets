# Reply Channel Pattern - Handover Documentation

**Date**: 2026-03-16
**Status**: In Progress - Pattern Identified, Refactoring Opportunity
**Topic**: Request-response communication pattern using oneshot channels

---

## Pattern Description

The codebase extensively uses a communication pattern called the **Reply Channel Pattern** (also known as **Request-Response Pattern** or **Ask Pattern** in actor terminology). This pattern enables synchronous code to communicate with asynchronous code by:

1. Creating a oneshot channel for the response
2. Sending a request with the response channel embedded
3. Blocking/awaiting on the response channel to get the result

---

## Current Implementation

### How It Works

The pattern appears throughout `stub_actor_runtime.rs` in methods like `open_read`, `aread`, `awrite`, and `aclose`:

```rust
// Example from stub_actor_runtime.rs:125-143
fn open_read(&self, _name: &str) -> isize {
    // 1. Create oneshot channel for response
    let (tx, rx) = oneshot::channel();

    // 2. Send request with embedded response channel
    if let Err(e) = self.system_tx.send(IoRequest::OpenRead {
        node_handle: self.node_handle,
        response: tx,  // ← Response channel embedded in request
    }) {
        error!(actor = ?self.node_handle, error = ?e, "open_read: failed to send request");
        return -1;
    }

    // 3. Block waiting for response
    let channel_handle = match rx.blocking_recv() {
        Ok(handle) => handle,
        Err(e) => {
            error!(actor = ?self.node_handle, error = ?e, "open_read: failed to receive response");
            return -1;
        }
    };

    // Use the response...
    // ...
}
```

### Request Structure

All `IoRequest` variants that need a response carry a `oneshot::Sender`:

```rust
// system_runtime.rs:159-207
pub enum IoRequest {
    OpenRead {
        node_handle: Handle,
        response: oneshot::Sender<ChannelHandle>,  // ← Reply channel
    },
    Read {
        handle: ChannelHandle,
        buffer: SendableBuffer,
        response: oneshot::Sender<isize>,  // ← Reply channel
    },
    Write {
        node_handle: Handle,
        std_handle: actor_runtime::StdHandle,
        data: Vec<u8>,
        response: oneshot::Sender<isize>,  // ← Reply channel
    },
    // ... etc
}
```

### Response Handling

The `SystemRuntime` receives requests, processes them asynchronously, then sends responses:

```rust
// system_runtime.rs:620-625
IoRequest::OpenRead { node_handle, response } => {
    self.handle_open_read(node_handle, response);
}
// Inside handler:
let channel_handle = self.alloc_channel_handle();
let _ = response.send(channel_handle);  // ← Send response back
```

---

## Why This Pattern

### Problem It Solves

1. **Sync-to-Async Bridge**: Actors run synchronously but I/O is async
2. **Type-Safe Response**: Each request type gets the correct response type
3. **Direct Communication**: No shared state or polling needed
4. **Backpressure**: Caller naturally blocks until response is ready

### Architecture Context

From `system_runtime.rs:10-70`, the codebase uses a specific sync-to-async bridge:

- Actors call blocking methods (`aread()`, `awrite()`)
- These send `IoRequest` to `SystemRuntime` via unbounded mpsc
- `SystemRuntime` processes requests asynchronously
- Actors block on `oneshot::Receiver::blocking_recv()` until response arrives

This allows multiple actors to make concurrent I/O requests without blocking the async runtime.

---

## Boilerplate Pattern

The pattern repeats 6+ times in `stub_actor_runtime.rs` with identical structure:

```rust
// Pattern repeated in:
// - open_read() (line 127)
// - open_write() (N/A - doesn't use pattern)
// - aread() (line 191, 235)
// - awrite() (line 304)
// - aclose() (line 350, 373)

let (tx, rx) = oneshot::channel();

if let Err(e) = self.system_tx.send(IoRequest::SomeVariant {
    /* ... params ... */
    response: tx,
}) {
    error!(/* ... */);
    return -1;
}

match rx.blocking_recv() {
    Ok(result) => result,
    Err(e) => {
        error!(/* ... */);
        return -1;
    }
}
```

---

## Available Solutions

### Option 1: Keep Current Implementation ✅ RECOMMENDED

**Pros:**
- Already works
- Uses standard `tokio::sync::oneshot` (tens of millions of downloads)
- Clear and explicit
- No external dependencies

**Cons:**
- Repetitive boilerplate
- Easy to forget error handling

**Decision**: This is already a good solution. The pattern is idiomatic Rust.

### Option 2: Helper Method

Add a helper to reduce boilerplate:

```rust
impl BlockingActorRuntime {
    fn request<T>(
        &self,
        make_request: impl FnOnce(oneshot::Sender<T>) -> IoRequest,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let (tx, rx) = oneshot::channel();
        self.system_tx.send(make_request(tx))?;
        Ok(rx.blocking_recv()?)
    }
}

// Usage:
fn open_read(&self, _name: &str) -> isize {
    let channel_handle = match self.request(|response| IoRequest::OpenRead {
        node_handle: self.node_handle,
        response,
    }) {
        Ok(h) => h,
        Err(e) => {
            error!(actor = ?self.node_handle, error = ?e, "open_read failed");
            return -1;
        }
    };
    // ...
}
```

**Pros:**
- Reduces repetition
- Centralizes error handling
- No external dependencies

**Cons:**
- Adds abstraction layer
- Makes pattern less explicit

### Option 3: Dedicated Crate - `reqchan`

**Crate**: [`reqchan`](https://crates.io/crates/reqchan)
**Purpose**: Request→response channel abstraction
**API**: `Requester` and `Responder` types

**Pros:**
- Purpose-built for this pattern
- Handles edge cases
- Work-sharing support (multiple responders)

**Cons:**
- Extra dependency
- Would require refactoring request/response model
- Overkill for current needs

### Option 4: Actor Framework

**Crates**:
- [`rsactor`](https://crates.io/crates/rsactor) - Lightweight with `AskHandler<M, R>`
- [`actix`](https://crates.io/crates/actix) - Full-featured, battle-tested
- [`ractor`](https://crates.io/crates/ractor) - Erlang-inspired, used by Meta

**Pros:**
- Built-in ask pattern
- Full actor lifecycle management
- Supervision trees, message routing, etc.

**Cons:**
- Major architectural change
- Heavy dependencies
- Current system already works

### Option 5: Tower Service Trait

**Crate**: [`tower`](https://crates.io/crates/tower)
**Pattern**: `Service` trait for async request→response

```rust
pub trait Service<Request> {
    type Response;
    type Error;
    type Future: Future<Output = Result<Self::Response, Self::Error>>;

    fn call(&mut self, req: Request) -> Self::Future;
}
```

**Pros:**
- Industry standard (Hyper, Tonic, Axum use it)
- Middleware/composition support
- Protocol-agnostic

**Cons:**
- Designed for async, not sync-to-async bridge
- Would require async `ActorRuntime` trait
- Breaking change to actor interface

---

## Recommendation

### Keep Current Implementation

**Rationale:**

1. **Already Correct**: The current pattern using `tokio::sync::oneshot` is idiomatic and standard
2. **No Breaking Changes**: Works with existing `ActorRuntime` trait
3. **Clear Ownership**: Each request owns its response channel
4. **Type Safe**: Compiler enforces correct response types

### Optional Enhancement: Helper Method

If boilerplate becomes a maintenance issue, consider adding the `request()` helper method (Option 2). This is a small, non-breaking addition that reduces repetition while keeping the pattern explicit.

**Do NOT:**
- Add actor framework dependencies
- Rewrite to use Tower (wrong abstraction level)
- Switch to `reqchan` (unnecessary dependency)

---

## Pattern Locations

### Current Usage

**File**: `src/stub_actor_runtime.rs`

| Method | Line | Request Type | Response Type |
|--------|------|--------------|---------------|
| `open_read()` | 127 | `OpenRead` | `ChannelHandle` |
| `aread()` (materialize) | 191 | `MaterializeStdin` | `ChannelHandle` |
| `aread()` (read) | 235 | `Read` | `isize` |
| `awrite()` | 304 | `Write` | `isize` |
| `aclose()` (reader) | 350 | `Close` | `isize` |
| `aclose()` (writer) | 373 | `CloseWriter` | `isize` |

**File**: `src/system_runtime.rs`

| Handler | Line | Response Sent |
|---------|------|---------------|
| `handle_open_read()` | 326 | `response.send(channel_handle)` |
| `handle_open_write()` | 361 | `response.send(channel_handle)` |
| `handle_read()` | 377-407 | Via `IoEvent::ReadComplete` |
| `handle_write()` | 432-458 | Via `IoEvent::SyncComplete` |
| `handle_close()` | 464-530 | Via `IoEvent::SyncComplete` |
| `handle_close_writer()` | 537-577 | Via `IoEvent::SyncComplete` |

---

## Related Patterns

### Fire-and-Forget Pattern

Some requests don't need responses:

```rust
// system_runtime.rs:638-647
IoRequest::ActorShutdown { node_handle } => {
    // No response channel - fire and forget
    self.dag.write().set_state(node_handle, NodeState::Terminating);
    self.pipe_pool.close_actor_writers(node_handle);
    self.dag.write().set_state(node_handle, NodeState::Terminated);
}
```

This is the **Tell Pattern** (vs **Ask Pattern** with response).

### Future-Based Response Pattern

For async operations, handlers return `IoFuture<K>` that eventually produces `IoEvent<K>`:

```rust
// system_runtime.rs:210-223
pub enum IoEvent<K: KVBuffers> {
    ReadComplete {
        handle: ChannelHandle,
        reader: MergeReader<K>,
        bytes_read: isize,
        response: oneshot::Sender<isize>,  // ← Reply channel preserved
    },
    SyncComplete {
        result: isize,
        response: oneshot::Sender<isize>,  // ← Reply channel preserved
    },
}
```

The response channel is carried through the async operation and used when the future completes.

---

## Educational Resources

### Terminology

- **Reply Channel Pattern**: Passing a channel for response in the request
- **Request-Response Pattern**: General term for synchronous-style communication
- **Ask Pattern**: Actor framework term (vs "Tell" for fire-and-forget)
- **Oneshot Channel**: Single-use channel for one value

### External References

**Crates:**
- [`tokio::sync::oneshot`](https://docs.rs/tokio/latest/tokio/sync/oneshot/) - Official docs
- [`reqchan`](https://crates.io/crates/reqchan) - Dedicated request-response crate
- [`request-channel`](https://crates.io/crates/request-channel) - Alternative implementation
- [`tower`](https://crates.io/crates/tower) - Service trait abstraction
- [`rsactor`](https://crates.io/crates/rsactor) - Lightweight actor framework with AskHandler
- [`actix`](https://github.com/actix/actix) - Full actor framework
- [`ractor`](https://github.com/slawlor/ractor) - Erlang-inspired actors

**Discussions:**
- [Rust Forum: 2-way (AKA request response) channel](https://users.rust-lang.org/t/2-way-aka-request-response-channel/71795)
- [Tokio Tutorial: Channels](https://tokio.rs/tokio/tutorial/channels)

---

## Key Insights

### Why This Pattern Fits Well

1. **Type Safety**: Each request type has compile-time associated response type
2. **No Shared State**: Each request carries its own response mechanism
3. **Natural Backpressure**: Caller blocks until async work completes
4. **Error Handling**: Channel errors indicate system failure (async runtime died)

### Pattern Strengths

- **Simple**: Easy to understand and debug
- **Composable**: Works with any request/response types
- **Standard**: Idiomatic Rust using Tokio primitives
- **Efficient**: Oneshot channels are optimized for single-use

### When NOT to Use

- **Fire-and-forget**: Use plain message passing (see `ActorShutdown`)
- **Streaming**: Use `mpsc` or other multi-value channels
- **Bidirectional**: Use separate channels for each direction

---

## Next Steps

**Immediate**: None - current implementation is good

**Future Considerations**:
1. If boilerplate becomes painful, add `request()` helper method
2. Document pattern in code comments for future maintainers
3. Consider whether `ActorRuntime` should be async (breaking change)

**Do NOT**:
- Add unnecessary dependencies
- Over-abstract the pattern
- Change working code without clear benefit

---

## Open Questions

- Should we add `request()` helper to reduce boilerplate?
- Is the error handling consistent across all uses?
- Should the pattern be documented in module-level comments?

---

**Current State**: Pattern identified and documented, no changes needed unless boilerplate becomes a maintenance burden.
