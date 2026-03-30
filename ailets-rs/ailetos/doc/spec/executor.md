# Executor: DAG Scheduling and Execution

## incremental-progress

Each execution step advances the DAG. Completed nodes are skipped, and the next ready node is executed.

## immediate-values

Constant values are available without execution. Value nodes provide their data immediately upon creation.

## on-demand-spawn

Actor spawning is deferred until input is available. This prevents premature resource allocation for nodes that may never execute.

## maximum-concurrency

The system runs as many actors concurrently as possible. Parallelism is limited only by spec://executor/executor.md#on-demand-spawn and dependency constraints.
