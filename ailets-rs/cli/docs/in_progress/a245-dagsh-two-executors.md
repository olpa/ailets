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

**CLI thread**: stays synchronous throughout. `join_handle` registers a `JoinWaiter` in a shared `Arc<Mutex>` and polls a one-shot `SyncSender<()>` with 50ms timeout, checking Ctrl+C between polls.

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

## What still needs to be done

### Next: Step 5 — `follow` (stream live output)

Stream a running node's stdout to the terminal without waiting for termination. `cat` only reads the completed KV buffer; `follow` must read incrementally while the actor is still running.

**Approach**: Open the pipe via `pipe_path(handle, StdHandle::Stdout)` and read in a loop, yielding to the tokio runtime between reads, until EOF. Run in a background task on `ailetos_rt`; cancel it on Ctrl+C. Add `follow <node>` command and document in help.

### After that: Step 9 — `wait terminated` event-based

Replace the 10ms poll loop in `wait terminated` with `join_handle` plus a deadline. The watcher already owns the termination signal; `cmd_wait "terminated"` should register a `JoinWaiter` with a timeout instead of spinning on the DAG state.

### `kill` — no generalization

`kill` stays `dbg`-only. The limitation is documented; no further work planned.

### `fg` is removed — update scripts

Any scripts using `fg` should replace it with `join <node>`.

## How to run tests

```
cargo test                         # run integration tests in cli/tests/shell.rs
cargo clippy -- -D warnings        # must be clean
```

Six integration tests currently pass:
- `execute_routes_output_through_sink` — help output goes through sink
- `run_completes_on_persistent_executor` — foreground value→cat pipeline terminates
- `multiple_bg_runs_are_allowed` — two simultaneous background runs succeed
- `background_termination_is_notified` — bg cat run prints `[cat#N] done` to notification sink
- `run_alias_completes` — `run <alias>` resolves and completes without hanging
- `foreground_run_suppresses_intermediate_notifications` — no noise during foreground run

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
