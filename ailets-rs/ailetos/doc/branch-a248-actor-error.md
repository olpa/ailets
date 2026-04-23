# Branch a248: Actor Error Codes

Spec reference: `spec://errors`

## Milestone: Error codes for terminated actors (no auto-close) ✓

- [x] Add `exit_code: i32` to `IoRequest::ActorShutdown` (0 = clean)
- [x] `ShutdownHandle`: store exit code, `mark_failed()` sets 130 (`EOWNERDEAD`)
- [x] `spawn_actor_task`: call `mark_failed()` on actor `Err`
- [x] `PipePool::close_actor_writers`: set `writer.set_error(e)` before close
- [x] `SystemRuntime` shutdown handler: pass exit code to `close_actor_writers`
- [x] `Node.exit_code: i32`: recorded on shutdown, shown in dag dump
- [x] Dag dump: red `✗ failed(N)` for non-zero exit code; hide suspended badge on terminated nodes
- [x] `dagsh kill [-N] <node>`: sends `ActorShutdown` with exit code N (default 130)
- [x] `is_ready_to_spawn`: return false when a dep terminated with non-zero exit code

## Deferred

- [x] Unit test: `close_actor_writers` with error code — reader sees error after data
  — `test_close_actor_writers_with_error_reader_sees_epipe` in `tests/pipe/pool.rs`

- [x] `writer-to-reader` EPIPE transformation (`spec://errors#writer-to-reader`)
  — `Reader::get_error()` now returns 32 when writer has non-zero errno
  — updated 3 existing tests + added `test_writer_error_transformed_to_epipe`
  — `src/pipe/reader.rs`

- [x] `reader-to-actor` propagation (`spec://errors#reader-to-actor`)
  — `IoRequest::Read` response now carries `(isize, i32)` (bytes_read, errno)
  — `aread()` stores errno in `last_read_errno: Arc<AtomicI32>` on failure
  — `mark_failed()` uses `last_read_errno` if set, else falls back to EOWNERDEAD
  — `MergeReader::get_error()` added; `handle_read` captures it after read
  — `tests/reader_to_actor.rs`: 3 tests covering get_errno, mark_failed with EPIPE/EOWNERDEAD

- [x] Per-call `get_errno` in `BlockingActorRuntime` (`src/stub_actor_runtime.rs`)
  — `get_errno()` returns `last_read_errno` (shared Arc with ShutdownHandle)

- [ ] Backward propagation (`spec://errors#backward-propagation`)
  — when all readers of a file close, writer receives 32 (`EPIPE`) on next write
  — requires: reader-count tracking in `PipePool`
