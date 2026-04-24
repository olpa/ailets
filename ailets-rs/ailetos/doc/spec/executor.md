# Executor: DAG Scheduling and Execution

## incremental-progress

Each execution step advances the DAG. Completed nodes are skipped, and the next ready node is executed.

## incremental-run

A run operates within an environment that persists between runs. Subsequent runs do not re-execute actors from the previous runs.

## immediate-values

Constant values are available without execution. Value nodes provide their data immediately upon creation.

## on-demand-spawn

Actor spawning is deferred until input is available. This prevents premature resource allocation for nodes that may never execute.

## maximum-concurrency

The system runs as many actors concurrently as possible. Parallelism is limited only by spec://executor/executor.md#on-demand-spawn and dependency constraints.

## actor-restart

An actor can be restarted at any time regardless of its current state, including while running or after terminating with an error. Output from the previous and new runs is merged.
