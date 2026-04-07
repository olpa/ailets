# Scheduler: DAG Execution Order and Control

## skip-terminated

Nodes that have already completed execution are not executed again. The scheduler skips nodes in `Terminated` state, enabling incremental builds.

## scheduler-ignores-suspended

The scheduler iterator yields nodes in topological order regardless of `Suspended` state. The scheduler's responsibility is determining execution order, not filtering based on runtime state.

Rationale: If a suspended node produced output before suspension, dependent nodes should still be able to process that output. If no output is available, the runtime will naturally block waiting for input.

## suspend-resume

Clients can suspend and resume individual nodes:
- **Suspend**: Pause a node, preventing further execution
- **Resume**: Restore a node to continue execution

These operations enable interactive control over node execution, similar to Erlang's process suspension. The runtime (not the scheduler) handles the actual pausing and resuming.

## data-flow-independence

Suspending a node:
- Does NOT block the scheduler from yielding dependent nodes
- Does NOT prevent dependents from consuming already-produced output
- DOES prevent the suspended node from further execution until resumed

Dependencies and dependents continue operating normally based on data availability.

## state-model

Nodes have the following states:

| State | Description |
|-------|-------------|
| `NotStarted` | Node has not been executed yet |
| `Running` | Node is currently executing |
| `Suspended` | Node is paused (runtime prevents execution) |
| `Terminating` | Node is shutting down |
| `Terminated` | Node has completed execution |

The `Suspended` state is a runtime concern - the scheduler treats it the same as `NotStarted` or `Running`.
