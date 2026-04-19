# On-Demand Actor Spawn — Handover

## Status: implemented, one bug remaining

All four planned steps are done and committed on branch
`a216-dagsh-background-exec`:

| Commit  | Change |
|---------|--------|
| bb3ddc3 | Lift `PipePool` out of `SystemRuntime` into `RunHandle::run` |
| ebfa591 | Add `spawn_notify` to `SystemRuntime`; fire on write/shutdown |
| a8c1957 | Replace sync spawn loop with async on-demand spawn loop |
| 1ca3170 | Remove `Terminated`-skipping filter from `SchedulerIter` |

## Remaining bug: spawn loop starved while CLI runs --bg commands

### Symptom

Running `scripts/concurrent_on_demand_start.dagsh` with `run --bg` shows
`shell_input` staying `pending` throughout all CLI commands. It only starts
(and immediately terminates) after `fg` is called. The job then hangs.

### Root cause

`run --bg` launches an async task (spawn loop + `SystemRuntime`) on the tokio
runtime. But CLI commands — `write`, `status`, `resume`, `show`, `close` —
execute synchronously on the foreground thread without yielding to the
executor. The spawn loop never gets a scheduling slot until `fg` blocks the
foreground thread.

Consequence: `shell_input` (which has no deps and is immediately ready) is
never spawned while the script runs. By the time `fg` unblocks the executor,
`close $src` has already been called. `shell_input` starts, finds its CLI
input queue closed, and terminates without writing anything to its stdout.

`dbg1` and `dbg2` were spawned earlier by the `resume` commands (those
apparently do yield briefly to the executor). They are blocked reading from
`shell_input`'s stdout, which was never realized. The job hangs.

### Fix

The CLI's `run --bg` command needs to yield to the tokio executor between
commands so the spawn loop can make progress. The minimal fix is to add a
short `tokio::task::yield_now().await` (or equivalent) after each CLI command
executes in background mode.

An alternative is to ensure `run --bg` uses a separate OS thread with its own
tokio runtime, so the spawn loop is truly independent of the CLI's execution.

### How to reproduce

From `ailets-rs/cli/`:

```
cargo run -- --load scripts/concurrent_on_demand_start.dagsh
```

Close stdin when the interactive prompt appears (Ctrl-D or pipe `echo ""`),
otherwise the shell waits for interactive input and never exits.

### What to observe

**Current (broken) output** — key lines to watch:

```
dagsh> run --bg
Started background run ...

dagsh> status
Nodes: 4 total, 4 pending, 0 running ...   ← shell_input should be running here

dagsh> write $src "he"
dagsh> status
Nodes: 4 total, 4 pending, 0 running ...   ← still pending, spawn loop never ran

dagsh> resume $dbg1
dagsh> status
Nodes: 4 total, 3 pending, 1 running ...   ← dbg1 running (resume yielded to executor)
                                              but shell_input still pending

dagsh> fg
Waiting for background job to complete...
shell_input actor starting
shell_input actor: channel closed, terminating  ← starts too late; src already closed
                                                   hangs here — dbg1/dbg2 starved
```

**Expected (fixed) output** — after the fix:

```
dagsh> run --bg
dagsh> status
Nodes: 4 total, 3 pending, 1 running ...   ← shell_input running immediately

dagsh> write $src "he"
dagsh> write $src "l"
dagsh> status
Nodes: 4 total, 2 pending, 1 running, 1 suspended ...  ← dbg1 suspended after 3B

... (intermediate states per script comments) ...

dagsh> fg
Job completed
dagsh> status
Nodes: 4 total, 0 pending, 0 running, 0 suspended, 4 terminated
```

### Files to change

| File | What changes |
|------|-------------|
| `cli/src/…` (background command dispatch) | yield to executor between commands |
