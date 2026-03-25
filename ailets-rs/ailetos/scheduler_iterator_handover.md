# Scheduler Iterator Issue - Handover Document

## Problem Statement

When using the `--one-step` flag in the DAG shell to execute nodes incrementally, the scheduler iterator does not make progress. Running `run --one-step` multiple times yields the same DAG state with no nodes being executed.

### Observed Behavior

```
dagsh> show
cat.4 [⋯ not built] # Step 3
└── cat.3 [⋯ not built] # Step 2
    └── cat.2 [⋯ not built] # Step 1
        └── value.1 [✓ built] # Input

dagsh> run --one-step
Running DAG from node 4...
DAG execution completed.

dagsh> show
cat.4 [⋯ not built] # Step 3  <-- No progress
└── cat.3 [⋯ not built] # Step 2
    └── cat.2 [⋯ not built] # Step 1
        └── value.1 [✓ built] # Input
```

### Expected Behavior

Each `run --one-step` should execute the next ready node in the DAG:
- First call: Execute `cat.2` (node 2)
- Second call: Execute `cat.3` (node 3)
- Third call: Execute `cat.4` (node 4)

## System Architecture Overview

### DAG Execution Model

The system uses a DAG (Directed Acyclic Graph) to model dependencies between nodes:
- **Concrete nodes**: Actual actors that execute and produce output
- **Alias nodes**: References to other nodes (traversed but not executed)
- **Value nodes**: Special nodes that output constant data

### Node States

Nodes transition through these states:
1. `NotStarted` - Initial state
2. `Running` - Being executed
3. `Terminating` - Shutting down
4. `Terminated` - Completed execution

### Value Nodes - Special Case

Value nodes are created with `NodeState::Terminated` at creation time because their output is static and known upfront. However, they still need their task to run to write the output data to the KV store.

See: `ailetos/src/environment.rs:111-121`

## Root Cause Analysis

### Investigation Steps

1. **Located the scheduler iterator** (`ailetos/src/scheduler.rs:117-151`)
   - Iterator builds a topological order of nodes
   - Yields nodes one by one
   - Applies stop conditions (`one_step`, `stop_before`, `stop_after`)

2. **Identified the issue**: The iterator yields ALL nodes in topological order, including those already in `Terminated` state

3. **Initial fix attempted**: Modified the iterator's `next()` method to skip already-terminated nodes:

```rust
loop {
    let node = *self.result.get(self.result_index)?;

    // Check if this node is already terminated
    if let Some(node_info) = self.dag.get_node(node) {
        if node_info.state == NodeState::Terminated {
            // Skip this node and continue to the next one
            self.result_index += 1;
            continue;
        }
    }

    // ... rest of logic ...
    return Some(node);
}
```

4. **Test verification**: Created test case that passes:
   - `test_one_step_skips_already_terminated_nodes` in `ailetos/tests/scheduler.rs:85-106`
   - Test marks node1 as Terminated, confirms iterator skips it and yields node2

## The Deeper Problem

### Value Node Lifecycle Contradiction

There's a fundamental architectural issue with how value nodes are handled:

1. **At creation time** (`environment.rs:116`):
   ```rust
   dag.set_state(handle, NodeState::Terminated);
   ```
   Value nodes are marked as `Terminated` because their output is conceptually "ready"

2. **At execution time** (`environment.rs:280-286`):
   ```rust
   let task = if let Some(value_data) = value_nodes.get(&node_handle).cloned() {
       Some(Self::spawn_value_node_task(...))  // Task still needs to run!
   }
   ```
   Value nodes still spawn tasks that write their data to storage

3. **The conflict**:
   - Iterator (correctly) skips `Terminated` nodes
   - Value nodes are `Terminated` but still need their tasks to run
   - Following nodes depend on value node output existing in storage
   - If value node task never runs, storage is never populated
   - Following nodes hang waiting for input that never arrives

### Why The System Hangs

When `run --one-step` is called:

1. Iterator builds order: `[value.1, cat.2, cat.3, cat.4]`
2. Iterator skips `value.1` (already Terminated)
3. Iterator yields `cat.2` with `one_step=true` flag
4. Task for `cat.2` spawns and tries to read from `value.1`'s stdout
5. **HANG**: `value.1`'s stdout doesn't exist in KV store because task never ran
6. System waits indefinitely for data that will never arrive

## Recommendations for Architects

### Option 1: Separate "State" from "Needs Execution"

Introduce a distinction between node state and whether a node needs its task executed:

```rust
pub struct Node {
    pub state: NodeState,
    pub needs_execution: bool,  // New field
    // ... other fields
}
```

- Value nodes: `state = Terminated`, `needs_execution = true` (first run only)
- After task runs once: `needs_execution = false`
- Iterator skips nodes where `needs_execution == false`

### Option 2: Different State for Value Nodes

Don't mark value nodes as `Terminated` at creation. Instead:

```rust
pub enum NodeState {
    NotStarted,
    ValueReady,     // New: value is known but not yet written
    Running,
    Terminating,
    Terminated,
}
```

- Value nodes start as `ValueReady`
- Iterator treats `ValueReady` as executable
- After task writes data, transitions to `Terminated`

### Option 3: Eager Value Node Execution

Write value node data to KV store immediately at creation time, before any DAG execution:

```rust
pub async fn add_value_node(&mut self, data: Vec<u8>, explain: Option<String>) -> Handle {
    let handle = /* ... create node ... */;

    // Immediately write data to KV store
    self.kv.write_value(handle, &data).await;

    dag.set_state(handle, NodeState::Terminated);
    // No need to spawn task later
}
```

- Pros: Simplest change, value nodes truly are "done" at creation
- Cons: Requires synchronous KV write at creation time

### Option 4: Mark Already-Run Nodes

Add tracking of which nodes have had their tasks spawned in previous runs:

```rust
// In Environment or Runtime
already_executed: HashSet<Handle>
```

- Iterator skips nodes in `already_executed` set
- After task completes, add to set
- Persists across multiple `run()` calls

## Recommended Approach

**Option 3 (Eager Value Node Execution)** appears most architecturally sound:

1. Value nodes are truly stateless constants
2. Writing them immediately is conceptually correct
3. Eliminates the state/execution contradiction
4. No changes needed to iterator logic
5. Clear separation: value nodes never spawn tasks

### Implementation Impact

**Files to modify:**
- `ailetos/src/environment.rs:111-121` - Add immediate KV write
- `ailetos/src/environment.rs:280-286` - Skip task spawning for value nodes

**Files to potentially remove code from:**
- `ailetos/src/environment.rs:185-220` - `spawn_value_node_task()` may become unnecessary

## Test Coverage

### New test added:
- `ailetos/tests/scheduler.rs:85-106` - Verifies iterator skips terminated nodes

### Additional tests needed:
1. End-to-end test with `--one-step` on multi-node pipeline
2. Test that value node data is available before dependent nodes run
3. Test partial execution and resumption

## Files Modified

1. **ailetos/src/scheduler.rs** - Added logic to skip already-terminated nodes
   - Lines 3, 117-163

2. **ailetos/tests/scheduler.rs** - Added test for skipping terminated nodes
   - Lines 3, 84-106

3. **ailetos/tests/environment.rs** - Fixed compilation error
   - Lines 14-15

## Current Status

- ✅ Iterator correctly skips terminated nodes (tested)
- ✅ Option 3 (Eager Value Node Execution) implemented
- ✅ `add_value_node` now writes data to KV storage immediately (async)

## Resolution

Option 3 was chosen and implemented. The `add_value_node` function is now `async` and writes value data to KV storage at creation time, eliminating the lifecycle contradiction.

---

**Document prepared by:** Claude Code
**Date:** 2026-03-23 (updated 2026-03-25)
**Related issue:** DAG iterator does not progress with `--one-step` flag
