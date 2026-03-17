# Stream Attachments

## overview

Attachments forward actor output to host stdout/stderr in real-time. They run as background tokio tasks that read from actor pipes and write to host streams.

## attachment-config

Configuration specifying which actors' stdout should be attached:

```rust
pub struct AttachmentConfig {
    stdout_actors: Vec<Handle>,
}
```

Methods:
- `attach_stdout(actor_handle)` - mark actor's stdout for attachment
- `should_attach_stdout(actor_handle)` - check if actor's stdout should be attached

## attachment-rules

Which streams get attached to which host output:

| StdHandle | Attachment behavior |
|-----------|---------------------|
| Stdout | Attach to host stdout **only if** actor is in `AttachmentConfig.stdout_actors` |
| Log | Always attach to host stderr |
| Metrics | Always attach to host stderr |
| Trace | Always attach to host stderr |
| Stdin | Never attach |
| Env | Never attach |

## attachment-lifecycle

Attachments are spawned **dynamically when writers are realized**, not eagerly on latent pipes.

### attachment-lifecycle.trigger

When `PipePool::touch_writer()` creates a new writer, the caller notifies `AttachmentManager::on_writer_realized()`.

### attachment-lifecycle.spawn

The attachment manager:
1. Checks attachment rules (see [attachment-rules](#attachment-rules))
2. If attachment is needed, spawns a tokio task
3. Task opens a reader via `pipe_pool.get_or_await_reader(key, false, id_gen)`
4. Task runs the read loop forwarding data to host stream

### attachment-lifecycle.read-loop

```
loop {
    n = reader.read(buf)
    if n > 0:
        write_all(buf[..n])
        flush()  // real-time streaming
    else if n == 0:
        break  // EOF
    else:
        break  // error
}
reader.close()
```

### attachment-lifecycle.shutdown

`AttachmentManager::waiting_shutdown()` awaits all spawned task handles.

## real-time-streaming

Attachments flush after every write to ensure real-time output. This is intentional - users expect to see output immediately, not buffered.
