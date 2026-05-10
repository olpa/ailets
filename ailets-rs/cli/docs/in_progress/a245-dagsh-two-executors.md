# dagsh refactor: two executors, live core

## Background

Previously dagsh was job-oriented: the user would start a job and wait for its completion. Each run created a fresh execution environment.

Now dagsh adopts a live core model, similar to the Erlang shell or Smalltalk: the execution environment is persistent and the user interface works directly with a running world. Runs are incremental — each run sees all state changes from previous runs.

## Architecture

**ailetos runtime**: a dedicated `tokio::runtime::Runtime` created at shell startup and owned by `DagShell` for the duration of the session. The `Environment` (DAG, KV storage, node states, pipe pool) lives inside this runtime and persists across runs.

**CLI**: the `rustyline` REPL remains synchronous. It communicates with the ailetos runtime via blocking channels. A CLI-side Tokio runtime is only introduced if async CLI work is needed.

**Executor lifecycle**: the executor exits when all actors in a run are done, but the environment persists. The next `run` is incremental and sees all prior node states and data. A "run forever until explicitly stopped" mode will be requested from the ailetos developers for future use.

## Commands

See [docs/commands.md](../commands.md).

## Implementation plan

### Approach

Red-green TDD throughout. Each step begins with a failing test that specifies the intended behaviour. Only the minimum code needed to pass the test is written; cleanup follows. No step is declared done until `cargo test` and `cargo clippy --deny warnings` both pass.

### Workflow per step

1. Write the tests in red state (failing, not yet passing).
2. Stop and let the developer review the tests. Commit after approval.
3. Before writing any implementation code, explain the planned approach and wait for the developer to confirm.
4. Implement the step.
5. Stop and let the developer review the implementation. Commit after approval.

### Step 0: Test harness

**Prerequisite for everything below.** `DagShell::execute` currently prints directly via `println!`, making output impossible to capture in unit tests.

Red: write a test that calls `shell.execute("help")` and asserts the returned value contains known text — it will not compile because `execute` returns `Result<bool, String>`, not output.

Green: introduce an `OutputSink` trait with a single method `println(&self, line: &str)`. Give `DagShell` a generic or boxed `OutputSink`. Production code passes a `StdoutSink`; tests pass a `CapturingSink` that collects lines into a `Vec<String>`. Thread the sink through all `cmd_*` methods that currently call `println!`.

This refactor is a prerequisite for every subsequent test.

### Step 1: Persistent ailetos runtime

Red:
```rust
#[test]
fn runtime_persists_across_commands() {
    let shell = DagShell::new_for_test();
    let h = shell.ailetos_rt.handle().clone();
    let (tx, rx) = std::sync::mpsc::channel();
    h.spawn(async move { tx.send(42u32).ok(); });
    assert_eq!(rx.recv().unwrap(), 42);
    // second use — same runtime, not a new one
    h.spawn(async move {});
}
```

Green: add `ailetos_rt: tokio::runtime::Runtime` to `DagShell`, initialised in `new()` with `tokio::runtime::Runtime::new().unwrap()`. `new_for_test()` is an alias for `new()` that also registers the test actors.

### Step 2: Run executor on the persistent runtime

Currently `run_foreground` and `run_background` each spawn a `std::thread` that creates its own `Runtime`. Replace both with a single helper that spawns `run_with_tx` as a task on `ailetos_rt`.

Red:
```rust
#[test]
fn run_completes_on_persistent_runtime() {
    let mut shell = DagShell::new_for_test();
    // simple value → cat pipeline
    shell.execute("node value hello").unwrap();
    shell.execute("node add cat").unwrap();
    shell.execute("dep $1 $0").unwrap();
    shell.execute("run $1").unwrap();
    let out = shell.execute("status $1").unwrap();
    assert!(out.contains("Terminated"));
}
```

Green: implement `spawn_run(handle, stop_conditions) -> RunHandle` that calls `ailetos_rt.spawn(run_with_tx(...))`. `run` (foreground) calls `spawn_run` then immediately calls the join logic (Step 4). `run --bg` calls `spawn_run` and stores the handle. Remove the per-run `std::thread::spawn` and per-run `Runtime::new`.

Verify: check that `executor.rs` calls `notification_queue.notify(handle, ...)` when a node transitions to `Terminated`. If it does not, add that call now — it is required by Steps 4 and 7.

### Step 3: Multiple concurrent background runs

Red:
```rust
#[test]
fn two_bg_runs_are_allowed() {
    let mut shell = DagShell::new_for_test();
    // two independent single-node pipelines
    // ...
    assert!(shell.execute("run $node_a --bg").is_ok());
    assert!(shell.execute("run $node_b --bg").is_ok());
}
```

Green: replace `bg_job: Option<BackgroundJob>` with `run_handles: Vec<RunHandle>` where `RunHandle` holds a `tokio::task::JoinHandle<()>` and a `CancellationToken` (from `tokio-util`). Remove the "background job already running" guard. `cmd_reset` drains `run_handles`, cancelling each token before reinitialising the environment.

### Step 4: `join` / `await` — wait for termination with output streaming

Red:
```rust
#[test]
fn join_blocks_until_terminated_and_streams_output() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(sink.clone());
    // value "hello" → cat
    // ...
    shell.execute("run $cat --bg").unwrap();
    shell.execute("join $cat").unwrap();
    let status = shell.execute("status $cat").unwrap();
    assert!(status.contains("Terminated"));
    assert!(sink.lines().iter().any(|l| l.contains("[") && l.contains("] hello")));
}
```

Green:
- `cmd_join(handle)`: open a read stream on the node's stdout pipe; spawn a reader task on `ailetos_rt` that forwards lines to `OutputSink` formatted as `[nodename] <line>`; subscribe to `NotificationQueueArc` for the handle; `block_on` the termination notification. On Ctrl+C, drop the subscription and return to the prompt — the node keeps running.
- Add `await` as a command alias that dispatches to `cmd_join`.

### Step 5: `follow` — stream output without waiting for termination

Red:
```rust
#[test]
fn follow_streams_without_blocking_on_termination() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(sink.clone());
    // shell_input node: stays running until explicitly closed
    // write "line1", follow, check sink contains "[nodename] line1"
    // node is NOT terminated after follow returns
    // ...
    let status = shell.execute("status $inp").unwrap();
    assert!(!status.contains("Terminated"));
}
```

Green: `cmd_follow(handle)`: same output reader as `cmd_join`, but the blocking wait is only on Ctrl+C or natural stream close — not on `Terminated`. On Ctrl+C, close the reader and return to the prompt.

### Step 6: `ExternalPrinter` for background output (non-blocking lines)

Steps 4 and 5 above use `OutputSink` while the REPL is blocked. This step wires `OutputSink` to rustyline's `ExternalPrinter` so that output can also arrive while the user is typing.

Red:
```rust
// integration-style: start a bg run, then call readline once;
// the CapturingExternalPrinter must have received the output line
// before readline returns.
```

Green:
- In `main()`, call `rl.create_external_printer()` and pass it to `DagShell::new`.
- `ExternalPrinterSink` wraps `rustyline::ExternalPrinter` and implements `OutputSink`.
- Replace `StdoutSink` with `ExternalPrinterSink` in the production path.
- The `CapturingSink` used in earlier tests is unchanged.

### Step 7: Background termination notifications

Red:
```rust
#[test]
fn background_node_termination_prints_notification() {
    let sink = CapturingSink::new();
    let mut shell = DagShell::new_with_sink(sink.clone());
    shell.execute("run $fast_node --bg").unwrap();
    // wait briefly for completion
    std::thread::sleep(std::time::Duration::from_millis(200));
    let lines = sink.lines();
    assert!(lines.iter().any(|l| l.contains("done") || l.contains("FAILED")));
}
```

Green: in `DagShell::new`, spawn a watcher task on `ailetos_rt`. The watcher holds a `Vec<broadcast::Receiver<_>>`, one per registered handle (populated via `notification_queue.subscribe`). When a `Terminated` event fires, it calls `sink.println("[nodename] done")` or `"[nodename] FAILED (exit N)"`. New handles are subscribed in `cmd_node_inner` when they are created.

### Step 8: Remove `fg`, update `kill`

Red:
```rust
#[test]
fn fg_is_removed() {
    let mut shell = DagShell::new_for_test();
    let err = shell.execute("fg").unwrap_err();
    assert!(err.contains("removed") || err.contains("unknown command"));
}

#[test]
fn kill_works_on_any_node() {
    let mut shell = DagShell::new_for_test();
    // run two independent nodes --bg; kill one; other continues
    // ...
}
```

Green: delete `cmd_fg`. Rewrite `cmd_kill`: look up the node state in `env.dag` directly; if running, call `IoBridge::cleanup_actor_io` via `ailetos_rt.block_on`. Remove all references to `bg_job` in `cmd_kill` and `cmd_reset`.

### Step 9: `wait` — replace poll loop with subscription

Red: existing `wait suspended` and `wait terminated` behaviour must be preserved. Add a test if none exists.

Green: replace the 10ms poll loop with `ailetos_rt.block_on(notification_queue.wait_async(handle, ...))`. Wrap in an `Abortable` future so Ctrl+C can interrupt the wait.

### Step 10: Delete dead code

Red: `cargo test && cargo clippy --deny warnings` must pass with zero warnings.

Green: delete `BackgroundJob`, `run_foreground`, `run_background`, and any remaining references to `bg_job`. Verify `cmd_reset` correctly cancels all `RunHandle` tokens and awaits their tasks before reinitialising the environment.

---

### ailetos dependency

Before starting Step 2, confirm that `executor.rs` calls `notification_queue.notify(handle, exit_code)` in the `ActorLifecycleEvent::Terminated` branch. If it does not, add that call as a prerequisite change to the ailetos crate. All subsequent steps that block on termination depend on this.

## Output handling and session exit

See [docs/dagsh.md](../dagsh.md) for user-facing behaviour. Implementation note: rustyline's `ExternalPrinter` is used for all node output so that notifications and streamed lines never corrupt the user's current input line.
