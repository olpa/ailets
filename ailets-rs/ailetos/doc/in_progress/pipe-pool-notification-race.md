# PipePool: Notification Race in `get_or_await_new_reader`

**Status:** Ready to fix — 1-file change in `ailetos/src/pipe/pool.rs`

## The Race

`get_or_await_new_reader` with `allow_latent=true` has a narrow but real race:

1. No entry exists → function creates `Latent(notify)` under lock, releases lock
2. Before `notify.notified().await` is first polled, `touch_writer` fires on another thread
3. `touch_writer` finds `Latent(notify)`, upgrades to `Realized`, calls `notify.notify_waiters()`
4. No waiter is registered yet → notification is lost (`notify_waiters` does not store a permit)
5. `get_or_await_new_reader` calls `notify.notified().await` → waits forever

The window is between lock release and first poll of `notified()`. On a multi-threaded tokio
runtime, another OS thread can hit `touch_writer` in this gap.

Note: the oneshot used in the reverted `attach_stdout_to` did not close this window — it fired
before `get_or_await_new_reader` was called, not before `notified().await` was registered.

## Fix Option A (preferred)

Replace `notify_waiters()` with `notify_one()` in `touch_writer` and `flush_close_actor_writers`.

`notify_one()` stores a permit. If no waiter is registered when it fires, the next `notified()`
call consumes the permit immediately. This closes the race without any structural change.

Check: each `(node, fd)` latent entry has exactly one `Notify`. Multiple concurrent readers on
the same pipe would each wait on the same `Notify` — `notify_one()` wakes only one. Verify
whether this matters in practice (fan-out through `AttachmentManager` creates independent tasks,
each calling `get_or_await_new_reader` separately, which each create their own latent entry via
the loop). If multiple tasks share one latent `Notify`, `notify_one()` is not sufficient and
Option B is needed.

## Fix Option B

Add `PipePool::ensure_latent(key)`: creates the latent entry synchronously under lock if no
entry exists. Call it from `attach_stdout_to` before spawning the consumer task. The consumer
then always enters `get_or_await_new_reader` with an existing entry.

## Related Bug: Value Nodes Bypass the Pool

`add_value_node` writes pipe data to KV via `write_completed_buffer` but does NOT insert a
`WriterState::Realized` entry into `PipePool.writers`. Calling `get_or_await_new_reader` for a
value node's pipe will create a latent entry and wait forever — the notification never fires
because no `touch_writer` is ever called.

This is a separate bug. Fix candidates:
- Have `add_value_node` insert a `Realized` entry into the pool after writing to KV
- Add a KV fallback in `get_or_await_new_reader` for terminated nodes
- Special-case value nodes in `attach_stdout_to`

## TDD Starting Point

```rust
// Failing test: attach AFTER value node is already created → must still receive data
let val = env.add_value_node(b"hello".to_vec(), None).await?;
// Pipe is in KV but NOT in PipePool.writers — get_or_await_new_reader will hang
env.attach_stdout_to(val, Box::new(sink));
executor.submit(val, StopConditions::default()).unwrap();
// assert sink received "hello"
```
