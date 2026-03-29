# Executor: DAG Scheduling and Execution

## incremental-progress

Each execution step advances the DAG. Completed nodes are skipped, and the next ready node is executed.

## immediate-values

Constant values are available without execution. Value nodes provide their data immediately upon creation.
