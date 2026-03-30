# A216: Start Actor On Demand

## Task

Implement on-demand actor spawning and maximum concurrency for the executor system.

## Requirements

### spec://executor/executor.md#on-demand-spawn

Actor spawning is deferred until input is available. This prevents premature resource allocation for nodes that may never execute.

### spec://executor/executor.md#maximum-concurrency

The system runs as many actors concurrently as possible. Parallelism is limited only by spec://executor/executor.md#on-demand-spawn and dependency constraints.

## Implementation Progress

### Phase 1: Debug Actor for Testing
- [ ] Create `deb` actor (debug cat variant)
  - Name: `deb`
  - Args: byte limit (e.g., `deb 100`)
  - Behavior: pass through N bytes, then pause
  - Control: stdin commands for resume
  - Logging: start/finish with actor ID via tracing
  - Pause: block waiting for resume command
  - Resume: continue until EOF

### Phase 2: Analysis
- [ ] Understand current spawning behavior in `Environment::spawn_actor_tasks`
- [ ] Identify how to detect when input becomes available for a node
- [ ] Design mechanism for triggering actor spawn on input availability
- [ ] Determine how to maintain maximum concurrency

### Phase 3: Implementation
- [ ] TODO

### Phase 4: Testing with deb actor
- [ ] Create DAG with multiple `deb` instances
- [ ] Verify on-demand spawning behavior
- [ ] Verify maximum concurrency
- [ ] Test pause/resume from dagsh

## Notes

