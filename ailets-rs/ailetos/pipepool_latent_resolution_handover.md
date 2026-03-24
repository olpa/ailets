# Pipepool Latent Resolution - Implementation Handover

## Specification References

This implementation addresses the following requirements:

- `spec://executor.md#incremental-progress` — Each execution step advances the DAG
- `spec://executor.md#immediate-values` — Constant values are available without execution
- `spec://pipe/pool.md#fulfillable-open` — Opening a pipe succeeds only when producer will eventually produce output

## Problem Context

When using `--one-step` flag for incremental DAG execution, the system hangs because:

1. Value nodes are marked `Terminated` at creation
2. The iterator (correctly) skips `Terminated` nodes
3. Value node tasks never run, so their data never reaches KV storage
4. Dependent nodes wait indefinitely for input that never arrives

## Solution Overview

Two changes are needed:

1. **Immediate values** (`spec://executor.md#immediate-values`): Write value node data to KV storage at creation time
2. **Fulfillable open** (`spec://pipe/pool.md#fulfillable-open`): Pipepool checks producer state and consults KV for terminated producers

## Part 1: Immediate Values

Write value node data to KV storage immediately in `add_value_node()`:

```rust
// In environment.rs, add_value_node()
pub fn add_value_node(&mut self, data: Vec<u8>, explain: Option<String>) -> Handle {
    let mut dag = self.dag.write();
    let handle = dag.add_node_with_explain("value".into(), NodeKind::Concrete, explain);

    dag.set_state(handle, NodeState::Terminated);

    // Write data to KV storage immediately
    // (implementation depends on KV API)
    self.kv.write_stdout(handle, &data);

    handle
}
```

**Files to modify:**
- `ailetos/src/environment.rs:111-121`

## Part 2: Fulfillable Open

### Current Behavior

When requesting an output pipe that doesn't exist:
- Create a latent pipe (blocks until data arrives)

### New Behavior

When requesting an output pipe that doesn't exist:

```
1. Check producer node state (exhaustive match):

   Match on NodeState:
   ├── NotStarted | Running | Terminating
   │   └── Create latent pipe (actor will produce output)
   │
   ├── Terminated
   │   └── Consult KV storage for existing output
   │       ├── Found → Return data from KV
   │       └── Not found → Error (producer will never produce output)
   │
   └── [Any future bad states]
       └── Error (fail fast, don't hang)
```

### Why This Order

| Scenario | Node State | Action | Result |
|----------|------------|--------|--------|
| Value node | `Terminated` | KV lookup | Data found (written at creation) |
| Running actor | `Running` | Latent pipe | Waits for fresh output |
| Completed actor | `Terminated` | KV lookup | Data from previous execution |
| Re-run (reset DAG) | `NotStarted` | Latent pipe | Fresh execution |

**Key principle:** A "good" actor (one that will produce output) takes precedence over KV data. This ensures re-runs produce fresh results, not cached data.

## Implementation Details

### Files to Modify

Primary file (location TBD - find Pipepool implementation):
- Look for pipe creation logic
- Find where latent pipes are created
- Add node state check and KV consultation

### Required Information for Pipepool

Pipepool needs access to:
1. **Node state** — to determine if actor will produce output
2. **KV storage** — to retrieve completed output

If Pipepool doesn't currently have this access, it may need:
- A reference to the DAG (for node state)
- A reference to KV storage (for data retrieval)

### Exhaustive Match Requirement

Use exhaustive match on `NodeState` to catch future state additions at compile time:

```rust
match node_state {
    NodeState::NotStarted | NodeState::Running | NodeState::Terminating => {
        // Actor will produce output - create latent pipe
        create_latent_pipe()
    }
    NodeState::Terminated => {
        // Check KV for existing output
        match kv.get_stdout(node_handle) {
            Some(data) => return_data(data),
            None => error("Producer will never produce output"),
        }
    }
    // No wildcard - compiler will catch new states
}
```

## Testing

### Test Cases

1. **Value node resolution** (`spec://executor.md#immediate-values`)
   - Create value node
   - Request its output via Pipepool
   - Verify data returned from KV (no latent pipe)

2. **Running actor resolution** (`spec://pipe/pool.md#fulfillable-open`)
   - Start actor (state = Running)
   - Request its output
   - Verify latent pipe created

3. **Completed actor resolution**
   - Run actor to completion
   - Request its output
   - Verify data returned from KV

4. **One-step execution** (`spec://executor.md#incremental-progress`)
   - Run `scripts/partial_run.dagsh`
   - Verify each `run --one-step` makes progress

### Integration Test

```
dagsh> show
cat.4 [⋯ not built]
└── cat.3 [⋯ not built]
    └── cat.2 [⋯ not built]
        └── value.1 [✓ built]

dagsh> run --one-step
# Should execute cat.2 (value.1 data comes from KV)

dagsh> show
cat.4 [⋯ not built]
└── cat.3 [⋯ not built]
    └── cat.2 [✓ built]  # Progress!
        └── value.1 [✓ built]
```

## Architectural Clarifications

1. **Error states**: Failed terminations (actor crashes) are handled by another system. Pipepool assumes `Terminating` nodes will produce output.

2. **Empty output**: Actors can legitimately produce zero bytes. The runtime creates the pipe regardless — Pipepool doesn't need to distinguish "no output" from "empty output."

3. **Scope**: "Open" from a node implies stdout only. Other outputs (stderr, custom streams) are not in scope.

## Open Questions

1. **Pipepool location:** Where is the Pipepool implementation? Need to locate the file.

2. **KV API:** What's the API for reading stdout data from KV storage?

3. **Access pattern:** Does Pipepool currently have access to DAG and KV, or does this need to be plumbed through?

## Summary

| Component | Change | Spec Reference |
|-----------|--------|----------------|
| `environment.rs` | Write value data to KV at creation | `spec://executor.md#immediate-values` |
| Pipepool | Check node state before creating latent pipe | `spec://pipe/pool.md#fulfillable-open` |
| Pipepool | Consult KV for `Terminated` nodes | `spec://pipe/pool.md#fulfillable-open` |
| Pipepool | Use exhaustive match on `NodeState` | `spec://pipe/pool.md#fulfillable-open` |

---

**Document prepared by:** Claude Code
**Date:** 2026-03-24
**Depends on:** `scheduler_iterator_handover.md`
