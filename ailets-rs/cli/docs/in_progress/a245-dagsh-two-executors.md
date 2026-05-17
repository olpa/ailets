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

| Commit | Step | Summary |
|--------|------|---------|
| `001b4eb` | 0 | `lib.rs` split + `OutputSink` trait + test harness |
| `dce74af` | 1–4 | Persistent `ailetos_rt` + `Executor`; `join`/`await` command; `fg` removed; multiple concurrent bg runs |
| `98ae79d` | 6–7 | Notification watcher thread; `OutputSink: Send + Sync`; `new_with_sinks` |
| `70763b8` | 6 | `ExternalPrinter` wired in `main.rs` via `ChannelSink` |

### What each commit delivers

**Step 0** (`lib.rs` split): Moves all `DagShell` implementation from `main.rs` into `lib.rs`. Adds `OutputSink` trait (`fn println(&self, line: &str)`). `StdoutSink` for production; `CapturingSink` (wraps `Arc<Mutex<Vec<String>>>`) in tests. All `println!` inside `DagShell` replaced with `self.sink.println(...)`. Adds `Cargo.toml` `[lib]` section; `tests/shell.rs` with first integration test.

**Steps 1–4**: Replaces `bg_job: Option<BackgroundJob>` and per-run `tokio::runtime::Runtime` with:
- `ailetos_rt: tokio::runtime::Runtime` — session-lifetime runtime
- `executor: Executor` — persistent, receives jobs via `submit`
- `events_rx: Receiver<ExecutorEvent>` — sync side of the bridge (later moved to watcher)
- `join_handle(target)` — polls with 50ms timeout; Ctrl+C detaches
- `cmd_join` / `cmd_await` commands
- `run --bg` submits and returns immediately; no single-slot constraint
- `cmd_reset` creates new env + executor on the same `ailetos_rt`; sends `WatcherUpdate`
- `node value` and `cat` reuse `ailetos_rt.block_on` instead of throwaway runtimes

**Steps 6–7**: Adds the notification watcher thread. `OutputSink` gains `Send + Sync` supertraits. `DagShell::new_with_sinks(command_sink, notification_sink)` separates the two sinks. `join_handle` registers a `JoinWaiter` in `pending_join` instead of reading `events_rx` directly.

## Known bug found during testing

**`run <alias>` hangs in foreground mode.**

When the target node is an alias (e.g. `set end = node alias .end $baz`), `cmd_run` calls `join_handle(alias_handle)`. The watcher registers `pending_join.target = alias_handle`. But the executor resolves the alias and emits `NodeTerminated(baz_handle)`. Since `baz_handle ≠ alias_handle`, the watcher prints it as a notification rather than signalling `join_handle`. The join waits forever.

**Fix**: resolve the alias before calling `join_handle`:

```rust
// in cmd_run, after executor.submit:
if !bg_flag {
    let join_target = self.env.resolve(handle);  // follow alias chain
    self.join_handle(join_target)?;
}
```

`env.resolve(h)` is already used in `attach_stdout_for_run` for the same reason. The fix is a one-liner in `cmd_run`.

## What still needs to be done

### Alias bug fix (required before merge)

Apply the one-liner fix above. Add a test:

```rust
#[test]
fn run_alias_completes() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(Box::new(sink.clone()));
    shell.execute("set v = node value hello").unwrap();
    shell.execute("set c = node add cat").unwrap();
    shell.execute("dep $c $v").unwrap();
    shell.execute("set end = node alias .end $c").unwrap();
    shell.execute("run $end").unwrap(); // must not hang
    let lines = sink.lines();
    assert!(lines.iter().any(|l| l.contains("built") || l.contains("Terminated")));
}
```

### Step 5: `follow` — stream live output

Stream actor output without waiting for termination. Requires a way to read from a node's stdout pipe incrementally (currently `cat` reads the whole KV buffer at once post-termination). This needs ailetos API support for streaming reads.

### Step 9: `wait` — event-based instead of polling

Replace the 10ms poll loop in `wait terminated` with an event-based wait:

```rust
// instead of thread::sleep loop:
loop {
    match self.events_rx.recv_timeout(poll_interval) {
        Ok(ExecutorEvent::NodeTerminated(h)) if h == handle => return Ok(()),
        Ok(_) => {}  // other node terminated, keep waiting
        Err(Timeout) => {}
        Err(Disconnected) => return Ok(()),
    }
    if Instant::now() >= deadline { return Err("Timeout..."); }
}
```

Note: `events_rx` is now owned by the watcher thread. To use it in `cmd_wait`, either:
a. Use `wait terminated` via `join_handle` (register a `JoinWaiter` + timeout wrapper), or
b. Add a separate subscription mechanism.

Option (a) is simpler: `cmd_wait "terminated"` can call `join_handle` with a deadline.

### Step 10: `kill` generalization

Currently `kill` only works for `dbg` nodes (uses `dbg_control::kill_dbg_actor`). Generalizing requires access to `IoBridge::cleanup_actor_io`, which is internal to the executor. Either expose it via a new `ExecutorEvent`/method on `Executor`, or accept the limitation for now.

### `fg` is removed — update scripts

Any scripts using `fg` should replace it with `join <node>`.

## How to run tests

```
cargo test                         # run integration tests in cli/tests/shell.rs
cargo clippy -- -D warnings        # must be clean
```

Four integration tests currently pass:
- `execute_routes_output_through_sink` — help output goes through sink
- `run_completes_on_persistent_executor` — foreground value→cat pipeline terminates
- `multiple_bg_runs_are_allowed` — two simultaneous background runs succeed
- `background_termination_is_notified` — bg cat run prints `[cat#N] done` to notification sink

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
