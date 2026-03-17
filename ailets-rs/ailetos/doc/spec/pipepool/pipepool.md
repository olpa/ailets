# PipePool

## overview

`PipePool` manages output pipes for actors. Each `(actor_handle, StdHandle)` pair can have its own output pipe. The pool handles latent pipe coordination and provides thread-safe access to writers and readers.

## data-structures

### data-structures.pool-inner

```rust
struct PoolInner {
    latent_writers: Vec<LatentWriter>,
    writers: Vec<(Handle, StdHandle, Arc<Writer>)>,
}
```

Note: readers are NOT stored - they are created on-demand and returned to callers.

### data-structures.pipepool

```rust
pub struct PipePool<K: KVBuffers> {
    inner: Mutex<PoolInner>,
    notification_queue: NotificationQueueArc,
    kv: Arc<K>,
}
```

## locking-strategy

### locking-strategy.single-mutex

All pipe state operations happen under `Mutex<PoolInner>`. This ensures atomic check-and-register operations:
- Consumer checks if writer exists AND registers latent waiter atomically
- Producer checks if latent exists AND notifies waiters atomically
- Shutdown extracts all waiters AND marks them closed atomically

### locking-strategy.notify-outside-lock

After extracting notify handles under lock, `notify_waiters()` is called AFTER releasing the lock. This prevents deadlock when notified readers immediately try to re-acquire the lock.

## api

### api.get-or-await-reader

```rust
pub async fn get_or_await_reader(
    &self,
    key: (Handle, StdHandle),
    allow_latent: bool,
    id_gen: &IdGen,
) -> Option<Reader>
```

See [latent-pipes.md#reader-creation](latent-pipes.md#reader-creation) for behavior.

### api.touch-writer

```rust
pub async fn touch_writer(
    &self,
    actor_handle: Handle,
    std_handle: StdHandle,
    id_gen: &IdGen,
) -> Result<(Arc<Writer>, bool), KVError>
```

Idempotent writer access. Returns `(writer, was_newly_created)`.

See [latent-pipes.md#writer-creation](latent-pipes.md#writer-creation) for behavior.

### api.close-actor-writers

```rust
pub fn close_actor_writers(&self, actor_handle: Handle)
```

Closes all writers (realized and latent) for an actor. Called on actor shutdown.

See [latent-pipes.md#shutdown-coordination](latent-pipes.md#shutdown-coordination) for behavior.

### api.get-already-realized-writer

```rust
pub fn get_already_realized_writer(
    &self,
    key: (Handle, StdHandle)
) -> Option<Arc<Writer>>
```

Returns writer only if already realized. Does not create latent entries.
