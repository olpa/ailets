# Scheduler: DAG Execution Order and Control

## skip-terminated

Nodes that have already completed execution are not executed again. The scheduler skips nodes in `Terminated` state, enabling incremental builds.

## skip-suspended

Nodes marked as `Suspended` are not eligible for execution. The scheduler skips suspended nodes until they are resumed.

A suspended node that was never started remains unscheduled. A suspended node that was running is paused.

## suspend-resume

Clients can suspend and resume individual nodes:
- **Suspend**: Pause a node, preventing it from executing
- **Resume**: Restore a node to its pre-suspension state

These operations enable interactive control over DAG execution, similar to Erlang's process suspension.

## dependency-independence

Suspending a node does not affect its dependencies. Dependencies continue executing normally. Dependents may block if waiting for the suspended node's output.

## state-model

Nodes have the following states:

| State | Description |
|-------|-------------|
| `NotStarted` | Node has not been executed yet |
| `Running` | Node is currently executing |
| `Suspended` | Node is paused, will not be scheduled |
| `Terminating` | Node is shutting down |
| `Terminated` | Node has completed execution |

The `Suspended` state is orthogonal to execution progress - a node can be suspended before starting or while running.
