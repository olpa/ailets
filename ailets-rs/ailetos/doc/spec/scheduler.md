# Scheduler: DAG Execution Order and Control

## skip-terminated

Nodes that have already completed execution are not executed again. The scheduler skips nodes in `Terminated` state, enabling incremental builds.

## scheduler-ignores-suspended

The scheduler iterator yields nodes in topological order regardless of whether they are suspended. The scheduler's responsibility is determining execution order, not filtering based on runtime state.

Rationale: If a suspended node produced output before suspension, dependent nodes should still be able to process that output. If no output is available, the runtime will naturally block waiting for input.

## suspend-resume

Clients can suspend and resume individual nodes:
- **Suspend**: Register the node for suspension; the node pauses at its next cooperative yield point (I/O operation)
- **Resume**: Unregister the node; execution continues from the next yield point

These operations enable interactive control over node execution, similar to Erlang's process suspension. The runtime (not the scheduler) handles the actual pausing and resuming via cooperative yield points.

## data-flow-independence

Suspending a node:
- Does NOT block the scheduler from yielding dependent nodes
- Does NOT prevent dependents from consuming already-produced output
- DOES prevent the suspended node from executing past its next yield point until resumed

Dependencies and dependents continue operating normally based on data availability.

## state-model

Nodes have the following execution states:

| State | Description |
|-------|-------------|
| `NotStarted` | Node has not been executed yet |
| `Running` | Node is currently executing |
| `Terminating` | Node is shutting down |
| `Terminated` | Node has completed execution |
