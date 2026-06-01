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

When a producer actor is killed, its downstream consumers may have spawned reader tasks via `pool.reader_future(...)` that are blocked inside `get_or_await_new_reader`, waiting on a `tokio::sync::Notify` for a latent pipe that will never be realized. Because `PipePool::flush_close_actor_writers` only closes writers for the *specific* actor that terminated, any latent pipes that were never created (e.g. because the producer was killed before it opened stdout) remain in `LatentState::Waiting` forever.

`prepare_exit()` never calls anything to drain these leftover latent entries, so the `JoinSet::join_next()` call blocks forever.

## Fix

Add a `PipePool::close_all_leftover_writers(errno: i32)` method that marks all remaining `LatentState::Waiting` entries as `LatentState::Closed(errno)` and fires their notifies. Call it from `prepare_exit()` (before the `join_next` drain loop) with `ECANCELED`.

Additionally, `LatentState::Closed` and `PipeError::PipeClosed` should carry an `i32` errno so callers can distinguish clean shutdown (errno=0) from error-induced closure.

## Files Affected

- `ailetos/src/pipe/pool.rs` — add `close_all_leftover_writers`; change `LatentState::Closed` and `PipeError::PipeClosed` to carry errno
- `ailetos/src/lib.rs` — export `ECANCELED`
- `cli/src/lib.rs` — call `close_all_leftover_writers(ECANCELED)` in `prepare_exit()`
