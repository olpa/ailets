# Pipe: Writer and Reader

## overview

In-memory pipe with async coordination via notification queue. Implements a broadcast-style pipe where one Writer appends to a shared buffer and multiple Readers can read at their own positions.

## writer

### writer.structure

```rust
pub struct Writer {
    shared: Arc<Mutex<SharedBuffer>>,
    handle: Handle,
    queue: NotificationQueueArc,
    debug_hint: String,
}
```

### writer.thread-safety

Writer is thread-safe via `parking_lot::Mutex`. Multiple threads can call write operations concurrently. The write lock is released before sending notifications.

### writer.write

POSIX-style write returning:
- Positive: bytes written
- 0: empty write (no notification sent)
- -1: error (closed, errno set, or buffer failed)

Empty writes do NOT notify observers to avoid unnecessary wakeups.

### writer.close

Closes the writer and notifies all readers via `queue.unlist(handle)`.

### writer.share-with-reader

Creates `ReaderSharedData` for spawning new readers:

```rust
pub struct ReaderSharedData {
    buffer: Arc<Mutex<SharedBuffer>>,
    writer_handle: Handle,
    queue: NotificationQueueArc,
}
```

## reader

### reader.structure

```rust
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

### reader.thread-safety

Reader safely accesses Writer's shared buffer via `Arc<Mutex>`. Multiple Readers can read from the same Writer simultaneously, each maintaining its own position.

The `read()` method takes `&mut self` - cannot call concurrently on the same Reader instance.

### reader.read

POSIX-style async read:
1. Check `should_wait_for_writer()` priority:
   - Error (own errno) → return -1
   - DontWait (data available) → proceed to read
   - Error (writer errno, caught up) → return -1
   - Closed (writer closed, caught up) → return 0 (EOF)
   - Wait → await notification, loop back
2. Read available data from buffer
3. Update position
4. Return bytes read

### reader.multiple-readers

Multiple independent Readers can read from the same Writer:
- Each Reader has its own position
- Each Reader maintains its own closed/errno state
- Readers do not interfere with each other
