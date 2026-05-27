# dagsh refactor: two executors, live core

## Background

Previously dagsh was job-oriented: the user would start a job and wait for its completion. Each run created a fresh execution environment.

Now dagsh adopts a live core model, similar to the Erlang shell or Smalltalk: the execution environment is persistent and the user interface works directly with a running world. Runs are incremental — each run sees all state changes from previous runs.

## Architecture (as implemented)

```
main thread (sync)
  │
  │  rustyline REPL
  │  DagShell::execute(cmd)
  │      │
  │      ├─ executor.submit(handle, stop_conds)   ← non-blocking
  │      │
  │      └─ join_handle(handle)                   ← polls, Ctrl+C escapes
  │              │  pending_join: Arc<Mutex<Option<JoinWaiter>>>
  │              │
  ╔══════════════╪════════════════════════════════════╗
  ║  ailetos_rt  │  (dedicated tokio Runtime)         ║
  ║              │                                    ║
  ║   Executor task ──► actor tasks (spawn_blocking)  ║
  ║        │                                          ║
  ║   lifecycle_event_task                            ║
  ║        │ NodeTerminated(h) via tokio mpsc          ║
  ║   Bridge task ──► sync mpsc (events_rx)           ║
  ╚══════════════╪════════════════════════════════════╝
                 │
          Notification watcher thread (OS thread, always running)
                 │
                 ├─ if pending_join.target == h  → signal JoinWaiter
                 └─ else                         → notification_sink.println("[name] done")
                                                          │
                                          ExternalPrinter (via ChannelSink)
                                          or StdoutSink fallback
```

**ailetos runtime** (`ailetos_rt: tokio::runtime::Runtime`): created once in `DagShell::new`, lives for the session. Owns the persistent `Executor` and all actor tasks. The `Environment` (DAG, KV storage, pipe pool) also lives on this runtime.

**Persistent Executor**: started once via `Executor::start(env, Some(events_tx))`. Jobs are submitted with `executor.submit(handle, stop_conditions)` (non-blocking). Multiple concurrent background runs are allowed — there is no single-job slot.

**Event bridge**: a tokio task on `ailetos_rt` forwards `ExecutorEvent` from a tokio channel to a `std::sync::mpsc` channel (`events_rx`). This decouples the async executor from the synchronous CLI.

**Notification watcher thread**: a permanent OS thread (not a tokio task) that owns `events_rx`. On each `NodeTerminated` event it either signals the active `join_handle` (if `pending_join` matches) or prints `[name#id] done` via the notification sink.

**CLI thread**: stays synchronous throughout. `join_handle(target, timeout)` registers a `JoinWaiter` in a shared `Arc<Mutex>` and polls a one-shot `SyncSender<()>` with 50ms timeout, checking Ctrl+C between polls. An optional deadline causes it to return `Err` instead of waiting indefinitely.

**Attachments / fan-out**: `AttachmentConfig` holds an unbounded list of custom sinks per node. When a writer is realized, `AttachmentManager::on_writer_realized` drains all sinks for that node and spawns one independent reader task per sink. Each task calls `pipe_pool.get_or_await_reader` independently, so multiple `follow` invocations (or colorized outputs) each receive a full copy of the data.

**ExternalPrinter**: `main()` creates a rustyline `ExternalPrinter` before building `DagShell`, wraps it in a `ChannelSink` (channel + background thread), and passes it as the notification sink. This ensures background `[name] done` lines are printed through the terminal's line-rewrite mechanism without corrupting in-progress user input.

## Key divergence from the original plan

The original plan was written against a `run_with_tx` API that was removed in master commit `9e491fa` (A253). That API was replaced by `Executor::start / submit / shutdown`. The branch was rebased to master HEAD before implementation started.

The original Step 2 planned to use `notification_queue` (an ailetos-internal subscription mechanism). The new API exposes events only via `events_tx: Option<mpsc::UnboundedSender<ExecutorEvent>>`. The watcher thread + `pending_join` mechanism replaces the planned `notification_queue` subscription approach.

## What was implemented (commits on this branch)

| Commit | Summary |
|--------|---------|
| `001b4eb` | `lib.rs` split + `OutputSink` trait + test harness |
| `dce74af` | Persistent `ailetos_rt` + `Executor`; `join`/`await` command; `fg` removed; multiple concurrent bg runs |
| `98ae79d` | Notification watcher thread; `OutputSink: Send + Sync`; `new_with_sinks` |
| `70763b8` | `ExternalPrinter` wired in `main.rs` via `ChannelSink` |
| `2069131` | Fix: resolve alias before `join_handle` so `run <alias>` doesn't hang |
| `8ae198e` | Fix: suppress intermediate notifications during foreground runs |
| `28bfbb9` | Fix: share `AttachmentConfig` live so `attach_stdout` works on persistent executor |
| `2605a4f` | Fix: use `pipe_path()` for `cmd_cat` so actor stdout is found in KV |
| `86c24ec` | Step 5: `follow` command — stream background node output via `ExternalPrinter` |
| `af58715` | Fix: `run --bg` attaches target stdout through `ExternalPrinter` |
| `64b3b6c` | Color support for `follow` and `run --bg` (256 named colors, ANSI 256-color) |
| `fc15e1c` | Fix: fan-out attachments — each `attach_stdout_to` gets its own reader |
| `4525094` | Step 9: event-based `wait terminated` via `join_handle` |

## Notes on completed work

### `kill` — no generalization

`kill` stays `dbg`-only. The limitation is documented; no further work planned.

### `fg` is removed — update scripts

Any scripts using `fg` should replace it with `join <node>`.

## What still needs to be done

> **Note:** The architecture description above documents the design as it was planned/partially implemented. The final implementation on this branch differs: `AttachmentManager`, `ChannelSink`, `pending_join`, and the OS-thread notification watcher were all replaced. See the actual source in `cli/src/lib.rs` and `cli/src/commands.rs` for the current design (two `tokio::Runtime` fields in `DagShell`: `ailetos_async_rt` and `cli_rt`).

### Known bugs — must fix before merge

#### 1. `quit` inside a sourced script does not propagate exit, leaves shell broken

**File:** `cli/src/commands.rs` (`cmd_source`) and `cli/src/lib.rs` (`execute`)

`execute("quit")` calls `prepare_exit()` (cancels the notification watcher) and returns `Ok(false)`. `cmd_source` catches `Ok(false)` and returns `Ok(())`. The outer `execute()` dispatch arm `"source" | "load" => self.cmd_source(rest)?` receives that `Ok(())`, falls through, and returns `Ok(true)`. The readline loop in `main.rs` continues as if nothing happened — but the notification watcher is now dead, so no `[node] done` lines will ever appear again.

**Fix:** Propagate the `Ok(false)` signal. One option: change `execute()` to return a tri-state (`Continue / Exit / Err`) so `cmd_source` can propagate `Exit` up. Alternatively, change `cmd_source` to return `Ok(false)` when the inner execute returns `Ok(false)`, and update the dispatch arm to `return self.cmd_source(rest)`.

---

#### 2. ~~Hang on exit when an actor is waiting for a pipe from a never-spawned upstream~~ — not reproducible

**Investigation result:** When `ailetos_async_rt` drops, Tokio drops all pending async tasks (it does not await them). Dropping an async task drops any oneshot senders it holds; blocking threads waiting on `blocking_recv` for those senders immediately receive `Err(RecvError)` and exit. This means the blocking thread pool drains cleanly without explicit latent-pipe cleanup.

A test (`shutdown_does_not_hang_with_latent_follow`) was written to reproduce the hang — a `follow` on a never-submitted node creates a `spawn_reader_to` task waiting on a latent pipe. The test passed in 0 ms, confirming no hang exists in practice.

---

### Known issues — should fix soon

#### 3. `spawn_reader_to` JoinHandle is dropped; last bytes may be lost on shutdown

**File:** `cli/src/commands.rs` (`cmd_follow` line ~358, `attach_one_node` line ~375)
**Pool docstring:** `ailetos/src/pipe/pool.rs` line 335

The docstring on `spawn_reader_to` says: *"Returns a JoinHandle the caller should `.await` after the executor shuts down to drain the last bytes."* Both call sites drop the handle immediately. The task is detached (keeps running), so on a normal exit the data usually drains before `ailetos_async_rt` shuts down. But in a fast-exit scenario (actor finishes just before quit), the runtime may shut down before the task is scheduled, silently truncating output.

**Fix:** Store the `JoinHandle` in `DagShell` (e.g., `Vec<JoinHandle<()>>`) and drain them in `prepare_exit` via `ailetos_async_rt.block_on(join_all(...))` before dropping the executor.

---

#### 4. `--one-step` with all nodes already terminated silently attaches nothing

**File:** `cli/src/commands.rs` (`attach_stdout_for_run` lines ~402–409)

When `--one-step` is used and every node in the DAG is already `Terminated`, `TopologicalOrderIter::find(|n| state == NotStarted)` returns `None` and `attach_one_node` is never called. No reader is spawned. The user gets no output and no error — just an empty line from `cmd_run`'s trailing `self.sink.println("")`.

**Fix:** When `find` returns `None`, either print a message like `"All nodes already completed"` or attach to the terminal node unconditionally so the user can use `cat` as a fallback path.

---

#### 5. `PipeClosed` in `spawn_reader_to` may leave a latent pipe entry unreleased

**File:** `ailetos/src/pipe/pool.rs` (`spawn_reader_to` lines ~354–359)

If `get_or_await_new_reader(allow_latent=true)` returns `Err(PipeClosed)` (writer closed before the reader task was scheduled), the task returns without calling `reader.close()`. If the pool had created a `Latent::Waiting` notifier entry for this key, that entry remains in the writers table indefinitely. Any future `spawn_reader_to` on the same `(handle, fd)` key waits on a notifier that never fires.

**Fix:** In the `Err(PipeClosed)` arm, explicitly remove or close the latent entry for the key, or ensure `get_or_await_new_reader` cleans up the latent entry before returning the error.

---

### Doc / maintenance

#### 6. Stale `attachment_manager` references in `ExecutorInfra` docstring

**File:** `ailetos/src/executor.rs` lines 429, 439, 448

The `ExecutorInfra` struct docstring still lists `attachment_manager` as a field and includes it as step 3 of the teardown sequence. The field and its shutdown step were removed when `attachments.rs` was deleted. A future developer adding a new resource between `io_bridge.shutdown()` and `drop(lifecycle_tx)` will misread the teardown order.

**Fix:** Delete the `attachment_manager` bullet from the field list and renumber the teardown steps to 1–4.

## How to run tests

```
cargo test                         # run integration tests in cli/tests/shell.rs
cargo clippy -- -D warnings        # must be clean
```

Nine integration tests currently pass:
- `execute_routes_output_through_sink` — help output goes through sink
- `run_completes_on_persistent_executor` — foreground value→cat pipeline terminates
- `multiple_bg_runs_are_allowed` — two simultaneous background runs succeed
- `background_termination_is_notified` — bg cat run prints `[cat#N] done` to notification sink
- `run_alias_completes` — `run <alias>` resolves and completes without hanging
- `foreground_run_suppresses_intermediate_notifications` — no noise during foreground run
- `two_follows_both_receive_output` — fan-out: each `follow` gets the full output
- `one_step_runs_first_pending_actor` — `run --one-step` runs exactly one actor
- `one_step_advances_past_terminated_nodes` — second `--one-step` skips already-done nodes

## File map

```
cli/
  src/
    lib.rs              ← DagShell, OutputSink, StdoutSink, all commands
    main.rs             ← main(), ExternalPrinter wiring, ChannelSink
    dbg_actor.rs        ← unchanged
    dbg_control.rs      ← unchanged
    shell_input_actor.rs ← unchanged
    shell_input_control.rs ← unchanged
  tests/
    shell.rs            ← CapturingSink + integration tests
  docs/
    commands.md         ← user-facing command reference
    dagsh.md            ← user-facing live core overview
    in_progress/
      a245-dagsh-two-executors.md  ← this file
```
