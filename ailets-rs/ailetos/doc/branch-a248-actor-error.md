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

- [x] Backward propagation (`spec://errors#backward-propagation`)
  — `SharedBuffer.reader_count` tracks live readers; `Reader::close()` decrements it
  — when count reaches 0 (writer still open, no prior error), sets `errno = EPIPE` on the shared buffer
  — 3 tests in `tests/pipe/pool.rs`: single reader, no readers, multiple readers

## Investigations ✓

### INV-1: Backward propagation not triggered when actor is killed ✓

**Observed:** After `kill -777 2` terminates `dbg.2`, `shell_input.1` keeps running.
Subsequent `write 1 "x"` succeeds and `shell_input.1` stays `⚙ running`.
EPIPE is never delivered to `shell_input.1`'s writer.

**Root cause hypothesis:**
`ActorShutdown` handler in `SystemRuntime` calls `close_actor_writers(node_handle,
exit_code)` — this closes dbg.2's *output* pipes and sets their error. But dbg.2's
*input* readers (the `MergeReader` held in the `channels` table as
`Channel::Reader(Option<MergeReader>)`) are never closed or dropped. The
`ReaderCountGuard` inside those readers keeps `reader_count > 0` on
`shell_input.1`'s stdout pipe, so `Writer::write()` never sees EPIPE.

**What needs to happen:**
When `ActorShutdown` is processed, all channel entries for that node that are
readers must be removed from `self.channels`, dropping the `MergeReader` and its
constituent `Reader`s, which drops their `ReaderCountGuard`s and triggers backward
propagation to upstream writers.

**Where to look:**
- `SystemRuntime::handle_actor_shutdown` (around line 658 of `src/system_runtime.rs`)
- `self.channels: HashMap<ChannelHandle, Channel<K>>`
- `Channel::Reader(Option<MergeReader<K>>)` — dropping this drops all `Reader`s
  inside `MergeReader`, which drops `ReaderCountGuard`s

**Key question:** Is there a clean mapping from `node_handle` to its channel entries,
or does the shutdown handler need to scan `self.channels` for entries belonging to
that node? Currently there is no per-node index into `channels`.

---

### INV-2: Shell hangs on exit after a dependency fails ✓

**Observed:** After `dbg.2` fails and `shell_input.1` closes, `cat.3` stays
`⋯ pending` forever. The shell hangs (or takes very long to exit) when the user
tries to quit.

**Root cause hypothesis:**
`run_with_tx` in `src/executor.rs` loops over `pending` nodes, waits on
`spawn_notify`, and only breaks when `pending` is empty (line 227). `cat.3`
is in `pending` but `is_ready_to_spawn` returns `false` for it (because its
dependency `dbg.2` terminated with a non-zero exit code). Since `cat.3` is never
removed from `pending` and never spawned, the loop never terminates.

`spawn_notify` fires when an actor shuts down (line 675), but after `shell_input.1`
terminates there are no more actors to fire it, and `pending` is still `[cat.3]`,
so the loop blocks forever on `spawn_notify.notified().await`.

The background thread therefore never exits. On shell exit the main thread calls
`thread.join()` (lines 458/463 of `cli/src/main.rs`) which blocks indefinitely.

**What needs to happen:**
Nodes that can never be spawned — because a dependency terminated with a non-zero
exit code — should be removed from `pending` (or immediately failed/skipped) so the
loop can terminate. Two options:

1. In the `is_ready_to_spawn` check, also detect the "blocked forever" case (all
   nodes in `pending` are permanently unspawnable) and break the loop.
2. When a node is detected as permanently unspawnable (failed dep), send an
   `ActorShutdown` with a suitable exit code (e.g. `ECANCELED`) to mark it
   terminated, then remove it from `pending`.

Option 2 is cleaner and consistent with how unregistered actors are handled
(lines 190-195 of `src/executor.rs`).

**Where to look:**
- `run_with_tx` loop, lines 175-233 of `src/executor.rs`
- `is_ready_to_spawn` in `src/executor.rs`
- The `spawn_notify.notified().await` at line 232
