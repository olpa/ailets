# Latent Pipes

## overview

Latent pipes solve the race condition between producer shutdown and consumer pipe opening. A latent pipe is a placeholder that exists before the actual writer is created.

## latent-state-machine

A pipe can be in one of these states:

```
[No Pipe] ─── reader requests ──→ [Latent/Waiting]
                                       │
            ┌─── writer created ───────┤
            │                          │
            ↓                          ↓ actor closes without writing
     [Realized]               [Latent/Closed]
            │                          │
            ↓ writer closes            ↓ readers get EOF
        [Closed]                   [Dropped]
```

### latent-state-machine.waiting

State when reader requests a pipe that doesn't exist yet. The reader blocks on a `tokio::sync::Notify` until the pipe is realized or closed.

### latent-state-machine.closed

State when actor terminates without ever writing to the pipe. All waiting readers receive EOF (return 0 from read).

## latent-writer-struct

```rust
pub struct LatentWriter {
    key: (Handle, StdHandle),
    state: LatentState,  // Waiting or Closed
    notify: Arc<tokio::sync::Notify>,
}
```

## reader-creation

Readers are created on-demand via `PipePool::get_or_await_reader()`:

1. If writer exists: create Reader immediately from `writer.share_with_reader()`
2. If latent writer exists and Waiting: await on notify, then loop back
3. If latent writer exists and Closed: return None (EOF)
4. If no entry and `allow_latent=true`: create LatentWriter, await, loop back
5. If no entry and `allow_latent=false`: return None

## writer-creation

Writers are created via `PipePool::touch_writer()`:

1. Fast path: return existing writer if already realized
2. Slow path: allocate buffer, create Writer
3. If latent entry existed: remove it, notify all waiters
4. Notification happens outside the lock to prevent deadlock

## shutdown-coordination

When an actor terminates, `PipePool::close_actor_writers()`:

1. For all latent writers of the actor: set state to Closed, collect notify handles
2. For all realized writers: call `writer.close()`
3. Release lock
4. Notify all collected waiters (outside lock)

This ensures readers never wait forever for a dead actor.
