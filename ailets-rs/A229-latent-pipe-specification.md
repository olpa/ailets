# A229: Latent Pipe Specification

## Overview

**Problem**: Race condition between lazy pipe creation and eager dependency reading causes pipes to not exist when readers look for them, breaking data flow.

**Solution**: Introduce **latent pipes** - pipes that exist in metadata form before writers connect. Readers (including attachments) can get latent pipes immediately and will block when reading until the pipe is realized by a writer.

## Core Concept

A **latent pipe** is a pipe that exists but has no writer or buffer yet. It transitions to **realized** when an actor writes, or to **closed** if the actor closes without writing.

```
Latent Pipe:        Created when reader/attachment needs it
   ↓                Readers block on read until realized or closed
   ↓                Attachments attach and wait
   ├→ Realized:     Writer connects and allocates buffer
   │                Readers/attachments unblock and data flows
   └→ Closed:       Actor closes without writing
                    Readers receive EOF and exit cleanly
```

**Key Features**:
- Attachments can attach to latent pipes and will start flowing data once realized
- No timeouts - proper shutdown signaling handles all cases
- Readers wait indefinitely but receive EOF when pipe is closed

## Design Choice: Enum Parameter

Use an explicit enum parameter on pipe access functions to control whether latent pipes can be created.

### Why Enum Over Boolean

- ✅ Self-documenting at call site
- ✅ Cannot confuse `true`/`false` meaning
- ✅ Extensible for future access modes
- ✅ Type-safe and clear intent

## Data Structures

### 1. PipeAccess Enum

```rust
/// Controls how pipe access behaves when pipe doesn't exist
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeAccess {
    /// Only access existing pipes
    /// Returns None if pipe doesn't exist
    ExistingOnly,

    /// Create a latent pipe if it doesn't exist
    /// Always returns Some (creates latent pipe on miss)
    OrCreateLatent,
}
```

### 2. PipeState Enum

```rust
/// State of a pipe - latent, realized, or closed
pub enum PipeState {
    /// Pipe exists but writer hasn't connected yet
    /// Reads will block until pipe is realized or closed
    Latent {
        /// Name of the pipe (for buffer allocation later)
        name: String,
        /// Notification queue for the pipe
        notification_queue: NotificationQueueArc,
        /// Notifier for when pipe becomes realized or closed
        realized_notify: Arc<tokio::sync::Notify>,
    },

    /// Pipe is fully realized with writer and buffer
    Realized {
        /// The writer side of the pipe
        writer: Writer,
        /// The backing buffer
        buffer: Arc<dyn Buffer>,
    },

    /// Pipe was closed without ever being realized
    /// Actor closed its output without writing
    ClosedWithoutData,
}
```

### 3. Pipe Structure

```rust
pub struct Pipe {
    /// Writer handle (may be placeholder for latent pipes)
    writer_handle: Handle,
    /// Shared state - either latent or realized
    state: Arc<Mutex<PipeState>>,
}

impl Pipe {
    /// Create a new latent pipe (no writer, no buffer)
    pub fn new_latent(
        name: String,
        notification_queue: NotificationQueueArc,
    ) -> Self {
        Self {
            writer_handle: Handle::placeholder(), // Temporary handle
            state: Arc::new(Mutex::new(PipeState::Latent {
                name,
                notification_queue,
                realized_notify: Arc::new(tokio::sync::Notify::new()),
            })),
        }
    }

    /// Create a new realized pipe (with writer and buffer)
    pub fn new_realized(
        writer_handle: Handle,
        notification_queue: NotificationQueueArc,
        name: String,
        buffer: Arc<dyn Buffer>,
    ) -> Self {
        let writer = Writer::new(
            writer_handle,
            notification_queue,
            &name,
            buffer.clone(),
        );

        Self {
            writer_handle,
            state: Arc::new(Mutex::new(PipeState::Realized {
                writer,
                buffer,
            })),
        }
    }

    /// Transition from latent to realized
    /// Allocates buffer and creates writer
    /// Wakes all readers waiting on this pipe (including attachments)
    pub async fn realize(
        &mut self,
        writer_handle: Handle,
        buffer: Arc<dyn Buffer>,
    ) {
        let mut state = self.state.lock();

        if let PipeState::Latent { name, notification_queue, realized_notify } = &*state {
            let writer = Writer::new(
                writer_handle,
                notification_queue.clone(),
                name,
                buffer.clone(),
            );

            // Transition state
            *state = PipeState::Realized { writer, buffer };

            // Update writer handle
            self.writer_handle = writer_handle;

            // Wake all waiting readers (including attachments)
            realized_notify.notify_waiters();
        }
        // If already realized, this is a no-op
    }

    /// Check if pipe is realized
    pub fn is_realized(&self) -> bool {
        matches!(&*self.state.lock(), PipeState::Realized { .. })
    }

    /// Get writer (only for realized pipes)
    pub fn writer(&self) -> Option<&Writer> {
        let state = self.state.lock();
        match &*state {
            PipeState::Realized { writer, .. } => Some(writer),
            PipeState::Latent { .. } => None,
        }
    }

    /// Get reader for this pipe
    /// Works for both latent and realized pipes
    /// Reader will block on read if pipe is latent
    pub fn get_reader(&self, reader_handle: Handle) -> Reader {
        Reader::new(reader_handle, Arc::clone(&self.state))
    }
}
```

## API Changes

### PipePool API

```rust
impl PipePool {
    /// Get a pipe with controlled latent pipe creation
    ///
    /// # Arguments
    ///
    /// * `actor_handle` - The actor that owns the pipe
    /// * `std_handle` - Which standard handle (Stdout/Stderr/Log)
    /// * `access` - Whether to create latent pipe if missing
    ///
    /// # Returns
    ///
    /// * `Some(PipeRef)` - Pipe exists (realized or latent)
    /// * `None` - Pipe doesn't exist and `access == ExistingOnly`
    ///
    /// # Examples
    ///
    /// ```rust
    /// // Get existing pipe only (for closing)
    /// let pipe = pool.get_pipe(handle, StdHandle::Stdout, PipeAccess::ExistingOnly)?;
    ///
    /// // Get or create latent pipe (for reading dependencies or attachments)
    /// let pipe = pool.get_pipe(handle, StdHandle::Stdout, PipeAccess::OrCreateLatent)?;
    /// ```
    pub fn get_pipe(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        access: PipeAccess,
    ) -> Option<PipeRef<'_>> {
        let mut pipes = self.pipes.lock();
        let key = (actor_handle, std_handle);

        match access {
            PipeAccess::ExistingOnly => {
                // Only return if exists
                if pipes.contains_key(&key) {
                    Some(PipeRef { guard: pipes, key })
                } else {
                    None
                }
            }
            PipeAccess::OrCreateLatent => {
                // Create latent if missing
                pipes.entry(key).or_insert_with(|| {
                    let name = format!("pipes/actor-{}-{:?}", actor_handle.id(), std_handle);
                    Pipe::new_latent(name, self.notification_queue.clone())
                });
                Some(PipeRef { guard: pipes, key })
            }
        }
    }

    /// Open a reader for a pipe (creates latent pipe if needed)
    ///
    /// This is the primary method for getting readers, including for attachments.
    /// If the pipe doesn't exist, a latent pipe is created and the reader will
    /// block until the pipe is realized.
    pub fn open_reader(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        id_gen: &IdGen,
    ) -> Option<Reader> {
        // Always use OrCreateLatent for readers
        let pipe_ref = self.get_pipe(actor_handle, std_handle, PipeAccess::OrCreateLatent)?;
        let reader_handle = Handle::new(id_gen.get_next());
        Some(pipe_ref.get_reader(reader_handle))
    }

    /// Realize a latent pipe (called on first write)
    ///
    /// If pipe doesn't exist, creates it as realized directly.
    /// If pipe is latent, transitions it to realized.
    /// If pipe is already realized, this is a no-op.
    ///
    /// # Errors
    ///
    /// Returns error if buffer allocation fails
    pub async fn realize_pipe(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        writer_handle: Handle,
    ) -> Result<(), KVError> {
        let name = format!("pipes/actor-{}-{:?}", actor_handle.id(), std_handle);

        // Check if pipe exists
        let mut pipes = self.pipes.lock();
        let key = (actor_handle, std_handle);

        if let Some(pipe) = pipes.get_mut(&key) {
            // Pipe exists - realize it if latent
            if !pipe.is_realized() {
                // Allocate buffer (outside lock)
                drop(pipes);
                let buffer = self.kv.open(&name, OpenMode::Write).await?;

                // Realize the pipe
                let mut pipes = self.pipes.lock();
                if let Some(pipe) = pipes.get_mut(&key) {
                    pipe.realize(writer_handle, buffer).await;
                }
            }
        } else {
            // Pipe doesn't exist - create as realized directly
            drop(pipes);
            let buffer = self.kv.open(&name, OpenMode::Write).await?;
            let pipe = Pipe::new_realized(
                writer_handle,
                self.notification_queue.clone(),
                name,
                buffer,
            );

            let mut pipes = self.pipes.lock();
            pipes.insert(key, pipe);
        }

        Ok(())
    }

    /// Create output pipe (legacy method - now realizes immediately)
    ///
    /// This maintains compatibility with existing code that creates
    /// pipes eagerly. New code should use realize_pipe instead.
    pub async fn create_output_pipe(
        &self,
        actor_handle: Handle,
        std_handle: StdHandle,
        name: &str,
        id_gen: &IdGen,
    ) -> Result<Handle, KVError> {
        let writer_handle = Handle::new(id_gen.get_next());
        self.realize_pipe(actor_handle, std_handle, writer_handle).await?;
        Ok(writer_handle)
    }
}
```

### Reader API

```rust
impl Reader {
    /// Create a reader from pipe state (latent or realized)
    pub fn new(reader_handle: Handle, pipe_state: Arc<Mutex<PipeState>>) -> Self {
        Self {
            handle: reader_handle,
            pipe_state,
        }
    }

    /// Read from pipe
    ///
    /// If pipe is latent, blocks until realized.
    /// If pipe is realized, reads normally.
    ///
    /// This blocking behavior works for all readers, including:
    /// - MergeReader reading from dependencies
    /// - Attachments reading for stdout/stderr forwarding
    pub async fn read(&mut self, buf: &mut [u8]) -> isize {
        // Wait for pipe to be realized if needed
        loop {
            let state = self.pipe_state.lock();

            match &*state {
                PipeState::Latent { realized_notify, .. } => {
                    // Clone notify handle before releasing lock
                    let notify = Arc::clone(realized_notify);
                    drop(state);

                    // Wait for realization
                    notify.notified().await;
                    // Loop back to check state again
                }
                PipeState::Realized { writer, .. } => {
                    // Get shared data for reading
                    let shared_data = writer.share_with_reader();
                    drop(state);

                    // Read from buffer
                    return shared_data.read(buf).await;
                }
            }
        }
    }

    // ... other methods (close, etc.)
}
```

## State Transitions

```
┌─────────────────────────────────────────────────────────┐
│  Pipe Lifecycle with Attachments                       │
└─────────────────────────────────────────────────────────┘

[No Pipe]
    │
    │ env.attach_stdout(node) called
    │ OR MergeReader needs dependency
    ↓
[Latent Pipe Created]
    │ - No writer
    │ - No buffer
    │ - Attachment spawned, starts reading (blocks)
    │ - MergeReader gets reader (blocks on read)
    │
    ├─── Actor opens stdout for writing
    │    ↓
    │   [Realized Pipe]
    │    │ - Writer exists
    │    │ - Buffer allocated
    │    │ - Attachment unblocks, starts forwarding data
    │    │ - MergeReader unblocks, starts reading data
    │    │ - Data flows through pipeline
    │    │
    │    │ All handles closed
    │    ↓
    │   [Pipe Dropped]
    │
    └─── Actor closes without writing
         ↓
        [ClosedWithoutData]
         │ - No data was written
         │ - Readers receive EOF (0)
         │ - Attachments exit cleanly
         ↓
        [Pipe Dropped]


Alternative path (actor writes before attachment):
┌──────────────────────┐
│ Actor writes first   │
└──────────────────────┘
    │
    │ realize_pipe() called, pipe doesn't exist
    ↓
[Realized Pipe Created Directly]
    │ - Skips latent state
    │ - Created as realized
    │
    │ env.attach_stdout() called later
    ↓
[Attachment Spawned on Realized Pipe]
    │ - Attachment starts immediately
    │ - No blocking needed
    └→ Data flows
```

## Usage Patterns

### Pattern 1: Reading from Dependencies (MergeReader)

```rust
impl MergeReader {
    fn create_next_reader(&mut self) -> Option<Reader> {
        let dep_handle = self.dep_iterator.next()?;

        // Create latent pipe if dependency hasn't written yet
        let pipe = self.pipe_pool.get_pipe(
            dep_handle,
            StdHandle::Stdout,
            PipeAccess::OrCreateLatent
        )?;

        // Get reader (works for both latent and realized pipes)
        let reader_handle = Handle::new(self.id_gen.get_next());
        Some(pipe.get_reader(reader_handle))
    }
}
```

### Pattern 2: Attachments (Key Feature!)

```rust
impl Environment {
    /// Attach actor's stdout to host stdout
    ///
    /// This can be called BEFORE the actor starts executing.
    /// If the pipe doesn't exist, a latent pipe is created.
    /// The attachment will block reading until the actor writes.
    pub fn attach_stdout(&mut self, node_handle: Handle) {
        self.pending_attachments.push((
            node_handle,
            actor_runtime::StdHandle::Stdout,
            crate::system_runtime::AttachmentConfig::Stdout,
        ));
    }
}

impl SystemRuntime {
    /// Spawn attachment immediately (no waiting for pipe creation)
    ///
    /// The attachment can be spawned on a latent pipe.
    /// It will block reading until the pipe is realized.
    fn spawn_attachment(
        &self,
        node_handle: Handle,
        std_handle: actor_runtime::StdHandle,
        config: AttachmentConfig,
    ) -> tokio::task::JoinHandle<()> {
        let pipe_pool = Arc::clone(&self.pipe_pool);
        let id_gen = Arc::clone(&self.id_gen);

        tokio::spawn(async move {
            // Open reader - creates latent pipe if needed
            // This always succeeds (latent pipe created on demand)
            let Some(reader) = pipe_pool.open_reader(node_handle, std_handle, &id_gen) else {
                warn!(node = ?node_handle, std = ?std_handle, "failed to open reader for attachment");
                return;
            };

            // Run attachment worker
            // If pipe is latent, first read() will block until realized
            match config {
                AttachmentConfig::Stdout => {
                    attach_to_stdout(node_handle, reader).await;
                }
                AttachmentConfig::Stderr => {
                    attach_to_stderr(node_handle, reader).await;
                }
            }
        })
    }

    /// Spawn all pending attachments at startup
    ///
    /// No need to wait for pipe_created notifications!
    /// Attachments spawn immediately and block on latent pipes.
    pub fn spawn_pending_attachments(&mut self) {
        for (node_handle, std_handle, config) in self.pending_attachments.drain(..) {
            debug!(node = ?node_handle, std = ?std_handle, "spawning attachment on latent pipe");
            let task = self.spawn_attachment(node_handle, std_handle, config);
            self.attachment_tasks.push(task);
        }
    }
}
```

**Key Insight**: With latent pipes, we can **eliminate the pipe_created notification system entirely**! Attachments just spawn immediately and wait for the pipe to be realized.

### Pattern 3: Writing (First Write Realizes Pipe)

```rust
impl SystemRuntime {
    fn handle_write(&mut self, handle: ChannelHandle, data: &[u8], ...) -> IoFuture<K> {
        if let Some(Channel::Writer { node_handle, std_handle }) = self.channels.get(&handle) {
            let pipe_pool = Arc::clone(&self.pipe_pool);
            let id_gen = Arc::clone(&self.id_gen);

            Box::pin(async move {
                // Realize pipe on first write
                let writer_handle = Handle::new(id_gen.get_next());
                if let Err(e) = pipe_pool.realize_pipe(
                    node_handle,
                    std_handle,
                    writer_handle
                ).await {
                    warn!("failed to realize pipe: {}", e);
                    return IoEvent::SyncComplete { result: -1, response };
                }

                // Now write to pipe
                let result = if let Some(pipe) = pipe_pool.get_pipe(
                    node_handle,
                    std_handle,
                    PipeAccess::ExistingOnly
                ) {
                    if let Some(writer) = pipe.writer() {
                        writer.write(data)
                    } else {
                        -1
                    }
                } else {
                    -1
                };

                IoEvent::SyncComplete { result, response }
            })
        }
    }
}
```

### Pattern 4: Closing Pipes

```rust
impl SystemRuntime {
    fn handle_close(&mut self, handle: ChannelHandle, ...) -> IoFuture<K> {
        if let Some(channel) = self.channels.remove(&handle) {
            match channel {
                Channel::Writer { node_handle, std_handle } => {
                    // Only access existing pipes for closing
                    if let Some(pipe) = self.pipe_pool.get_pipe(
                        node_handle,
                        std_handle,
                        PipeAccess::ExistingOnly
                    ) {
                        if let Some(writer) = pipe.writer() {
                            writer.close();
                        }
                    }
                    // No warning if pipe doesn't exist - it's OK if never written
                }
            }
        }
    }
}
```

## Simplified Architecture

### Before (Complex)

```
1. Register attachment request
2. Wait for actor to start
3. Wait for first write
4. Pipe created → send pipe_created notification
5. SystemRuntime receives notification
6. Spawn attachment task
7. Attachment starts reading
```

### After (Simple)

```
1. Register attachment request
2. Spawn attachment immediately on latent pipe
3. Attachment blocks reading
4. Actor starts and writes
5. Pipe realized → attachment unblocks
6. Data flows to host
```

**Eliminated**:
- ❌ `pipe_created` notification channel
- ❌ `pipe_created_rx` receiver
- ❌ `pipe_created_tx` sender
- ❌ `handle_pipe_created()` method
- ❌ Complex notification routing

**Benefits**:
- ✅ Simpler control flow
- ✅ Fewer moving parts
- ✅ No race conditions
- ✅ Immediate attachment spawning

## Migration Strategy

### Phase 1: Add Latent Pipe Infrastructure
1. Add `PipeAccess` enum to `pipepool.rs`
2. Add `PipeState` enum to `pipe.rs`
3. Modify `Pipe` struct to use `PipeState`
4. Implement `Pipe::new_latent()` and `Pipe::realize()`

### Phase 2: Update PipePool API
1. Add `access: PipeAccess` parameter to `get_pipe()`
2. Implement `realize_pipe()` method
3. Update `open_reader()` to always use `OrCreateLatent`
4. Update all call sites to pass `PipeAccess::ExistingOnly` (maintain current behavior)

### Phase 3: Update Reader
1. Modify `Reader` to accept `Arc<Mutex<PipeState>>`
2. Implement blocking behavior in `Reader::read()` for latent pipes
3. Test that readers block and unblock correctly

### Phase 4: Enable Latent Pipes for Dependencies
1. Change `MergeReader::create_next_reader()` to use `PipeAccess::OrCreateLatent`
2. Change first write in `handle_write()` to call `realize_pipe()`
3. Remove lazy pipe creation from `handle_write()`

### Phase 5: Simplify Attachment System
1. Change `spawn_pending_attachments()` to spawn immediately on latent pipes
2. Remove `pipe_created` notification channel from `SystemRuntime::new()`
3. Remove `pipe_created_rx` from main event loop
4. Remove `handle_pipe_created()` method
5. Remove `pipe_created_tx` from `PipePool`
6. Remove `drop_pipe_created_tx()` method

### Phase 6: Cleanup
1. Remove warnings for "pipe not found" in close handler (expected behavior now)
2. Update tests to account for latent pipe behavior
3. Update documentation
4. Remove dead code related to pipe notifications

## Error Handling

### Actor Failure and Shutdown

When an upstream actor fails or completes without writing, the proper behavior is:

1. **Actor clears its fd table** during shutdown (via `shutdown()`)
2. **SystemRuntime closes all pipes** for the actor (via `pipe_pool.close_actor_writers()`)
3. **Writer close propagates EOF** to all readers on that pipe
4. **Readers (including attachments) receive EOF** and exit cleanly

**No timeouts needed** - the system handles failure through proper shutdown signaling.

### Latent Pipe Shutdown

If a latent pipe is never realized (actor never writes):

```rust
impl Writer {
    /// Close writer on latent pipe
    /// Transitions pipe to "closed without data" state
    pub fn close(&mut self) {
        let mut state = self.pipe_state.lock();

        match &mut *state {
            PipeState::Latent { realized_notify, .. } => {
                // Pipe was never realized - notify readers with EOF
                *state = PipeState::ClosedWithoutData;
                realized_notify.notify_waiters();
            }
            PipeState::Realized { .. } => {
                // Normal close
                self.close_buffer();
            }
        }
    }
}

impl Reader {
    pub async fn read(&mut self, buf: &mut [u8]) -> isize {
        loop {
            let state = self.pipe_state.lock();

            match &*state {
                PipeState::Latent { realized_notify, .. } => {
                    let notify = Arc::clone(realized_notify);
                    drop(state);

                    // Wait indefinitely - no timeout
                    // Either pipe is realized or closed
                    notify.notified().await;
                    // Loop to check new state
                }
                PipeState::Realized { writer, .. } => {
                    let shared_data = writer.share_with_reader();
                    drop(state);
                    return shared_data.read(buf).await;
                }
                PipeState::ClosedWithoutData => {
                    // Actor closed without writing
                    return 0; // EOF
                }
            }
        }
    }
}
```

### Buffer Allocation Failures

```rust
impl PipePool {
    pub async fn realize_pipe(...) -> Result<(), KVError> {
        let buffer = self.kv.open(&name, OpenMode::Write).await?;
        // Error propagates to caller (handle_write)
        // Writer will fail, actor will see write error
        // Actor should handle error and close cleanly
    }
}
```

## Testing Considerations

### Test Cases

1. **Latent → Realized transition**
   - Create latent pipe
   - Start reader (should block)
   - Realize pipe
   - Verify reader unblocks and reads data

2. **Attachment on latent pipe**
   - Spawn attachment before actor starts
   - Verify attachment blocks
   - Actor writes data
   - Verify attachment receives and forwards data to host

3. **Attachment on realized pipe**
   - Actor writes before attachment spawns
   - Spawn attachment
   - Verify attachment reads immediately without blocking

4. **Multiple readers on latent pipe**
   - Create latent pipe
   - Attach multiple readers (all block)
   - Realize pipe
   - Verify all readers unblock

5. **Latent pipe closed without writing**
   - Create latent pipe
   - Start reader (blocks)
   - Actor closes without writing (ClosedWithoutData)
   - Verify reader receives EOF (0) and exits cleanly

6. **Close unrealized pipe**
   - Create latent pipe
   - Close without writing
   - Verify no warnings/errors
   - Verify readers receive EOF

7. **End-to-end data flow**
   - Build pipeline: actor1 → actor2 → actor3
   - Attach stdout on actor3
   - All pipes start latent
   - Verify data flows through entire pipeline to host

8. **Actor failure before writing**
   - Create latent pipe with attachment
   - Actor fails during initialization (before any write)
   - Actor closes handles cleanly
   - Verify attachment receives EOF and exits

## Benefits

1. **Eliminates race condition**: Pipes always exist when readers look for them
2. **Preserves lazy allocation**: Buffers allocated only on first write
3. **Explicit control**: `PipeAccess` parameter makes intent clear
4. **No data flow breakage**: Readers block until data available
5. **Simplified attachment system**: No notification channel needed
6. **Immediate attachment spawning**: Attachments work on latent pipes
7. **Clean shutdown**: Actors close cleanly, readers receive proper EOF
8. **No timeouts**: System handles failures through proper signaling
9. **Fewer moving parts**: Less code, less complexity

## Files to Modify

### Core Changes
1. `ailetos/src/pipepool.rs` - Add `PipeAccess` enum, update `get_pipe()`, add `realize_pipe()`
2. `ailetos/src/pipe.rs` - Add `PipeState` enum, update `Pipe` struct
3. `ailetos/src/merge_reader.rs` - Use `PipeAccess::OrCreateLatent`
4. `ailetos/src/system_runtime.rs` - Update write handler to realize pipes, update close handler
5. `ailetos/src/lib.rs` - Export `PipeAccess` enum

### Simplification (Remove notification system)
6. `ailetos/src/system_runtime.rs` - Remove `pipe_created_rx`, `handle_pipe_created()`, spawn attachments immediately
7. `ailetos/src/pipepool.rs` - Remove `pipe_created_tx`, `drop_pipe_created_tx()`
8. `ailetos/src/environment.rs` - Update attachment spawning

---

**Specification Version**: 1.1
**Date**: 2026-03-04
**Status**: Ready for Implementation
**Branch**: a229-attach-host-stdout
**Key Feature**: Attachments work on latent pipes
