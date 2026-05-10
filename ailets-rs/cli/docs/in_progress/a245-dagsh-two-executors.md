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

## Output handling and session exit

See [docs/dagsh.md](../dagsh.md) for user-facing behaviour. Implementation note: rustyline's `ExternalPrinter` is used for all node output so that notifications and streamed lines never corrupt the user's current input line.
