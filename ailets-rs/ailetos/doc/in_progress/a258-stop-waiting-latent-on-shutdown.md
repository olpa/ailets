# dagsh hangs on quit after actor is killed (latent pipe reader never unblocked)

## Summary

When a `dagsh` script kills an actor mid-run (e.g. via `kill`) and then the user types `quit`, the CLI hangs indefinitely in `prepare_exit()`.

## Reproduction

Run `scripts/kill_actor.dagsh`, then type `quit`.

## Root Cause

`prepare_exit()` in `cli/src/lib.rs` drains a `JoinSet` of reader tasks before shutting down:

```rust
rt.block_on(async { while tasks.join_next().await.is_some() {} });
```

When a producer actor is killed, its downstream consumers may have spawned reader tasks via `pool.reader_future(...)` that are blocked inside `get_or_await_new_reader`, waiting on a `tokio::sync::watch` channel for a latent pipe that will never be realized. Because `PipePool::flush_close_actor_writers` only closes writers for the *specific* actor that terminated, any latent pipes that were never created (e.g. because the producer was killed before it opened stdout) remain in `LatentState::Waiting` forever.

`prepare_exit()` never calls anything to drain these leftover latent entries, so the `JoinSet::join_next()` call blocks forever.

## Fix (implemented)

Added `PipePool::close_all_leftover_writers()` in `ailetos/src/pipe/pool.rs`. It sweeps all entries still in `LatentState::Waiting`, marks them `LatentState::Closed`, and fires their notifiers. Called from `prepare_exit()` in `cli/src/lib.rs` before the `join_next` drain loop.

The `errno` parameter proposed in the original issue was dropped: `LatentState::Closed` and `PipeError::PipeClosed` do not carry errno, as no current caller uses the distinction. A follow-on issue can add it when there is a concrete consumer.

## Files Changed

- `ailetos/src/pipe/pool.rs` — added `close_all_leftover_writers()`
- `cli/src/lib.rs` — call `close_all_leftover_writers()` in `prepare_exit()` before the drain loop
- `ailetos/tests/pipe/pool.rs` — test: latent reader unblocks with `PipeClosed` after the call
