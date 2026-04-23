# Branch a248: Actor Error Codes

Spec reference: `spec://errors`

## Milestone: Error codes for terminated actors (no auto-close)

### Tasks

- [x] Add `exit_code: Option<i32>` to `IoRequest::ActorShutdown`
  — `src/system_runtime.rs`

- [x] `ShutdownHandle`: store exit code, expose `mark_failed()`
  — `src/stub_actor_runtime.rs`
  — add `exit_code: Arc<AtomicI32>` (default 0 = clean)
  — `mark_failed()` sets 130 (`EOWNERDEAD`)
  — `do_shutdown()` reads and forwards exit code

- [x] `spawn_actor_task`: call `shutdown.mark_failed()` on actor `Err`
  — `src/executor.rs`

- [x] `PipePool::close_actor_writers`: accept and apply `exit_code: Option<i32>`
  — `src/pipe/pool.rs`
  — for realized writers: call `writer.set_error(e)` before `writer.close()`

- [x] `SystemRuntime` shutdown handler: pass exit code to `close_actor_writers`
  — `src/system_runtime.rs`

### Deferred

- [ ] `writer-to-reader` EPIPE transformation (`spec://errors#writer-to-reader`)
  — readers currently see the writer's errno (e.g. 130) instead of 32 (`EPIPE`)
  — `Reader::get_error()` should return 32 when writer errno is non-zero
  — `src/pipe/reader.rs`

- [ ] `reader-to-actor` propagation (`spec://errors#reader-to-actor`)
  — when actor reads 32 (`EPIPE`), it should fail and its output files close with 32
  — requires: accurate `get_errno()` in `BlockingActorRuntime`

- [ ] Backward propagation (`spec://errors#backward-propagation`)
  — when all readers of a file close, writer receives 32 (`EPIPE`) on next write
  — requires: reader-count tracking in `PipePool`

- [ ] Per-call `get_errno` in `BlockingActorRuntime` (`src/stub_actor_runtime.rs`)
  — currently always returns 0
  — needed for actors to distinguish error types after failed reads/writes
