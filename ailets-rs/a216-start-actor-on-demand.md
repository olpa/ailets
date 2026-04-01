# A216: Start Actor On Demand

> **Purpose:** This is a Work Activity Log (WAL) for task A216. It provides context and plan for resuming work after interruptions or crashes. Implementation details are in git commits, not here.

## Task

Implement on-demand actor spawning and maximum concurrency for the executor system.

## Requirements

### spec://executor/executor.md#on-demand-spawn

Actor spawning is deferred until input is available. This prevents premature resource allocation for nodes that may never execute.

### spec://executor/executor.md#maximum-concurrency

The system runs as many actors concurrently as possible. Parallelism is limited only by spec://executor/executor.md#on-demand-spawn and dependency constraints.

## Current State

### Phase 1: Debug Actor for Testing ✅ COMPLETE

**Purpose:** Create a controllable actor to test on-demand spawning behavior.

**What was built:**
- `dbg` actor: passes through N bytes (default 100), then pauses
- Control system: `resume <node>` command to unpause actors
- Integrated into CLI at `cli/src/dbg_actor.rs` and `cli/src/dbg_control.rs`

**Key architectural decisions:**
- Actor signature changed: `ActorFn = fn(BlockingActorRuntime) -> Result<(), String>`
  - Actors receive full runtime, can call `runtime.node_handle()` to get their ID
  - Enables self-configuration pattern (actors look themselves up in registries)
- Removed thread-local storage from dbg_control
  - Now uses only global registry: `HashMap<Handle, Arc<DbgControl>>`
  - Actor looks up control by its node handle
- Test script: `cli/scripts/test_dbg.dagsh`

**Latest commit:** 85e0a5a "A216 Refactor actor signature to pass runtime, remove thread-locals"

### Phase 2: Analysis 🔄 NEXT

Goal: Understand current spawning and design on-demand mechanism.

Tasks:
- [ ] Understand current spawning behavior in `Environment::spawn_actor_tasks`
- [ ] Identify how to detect when input becomes available for a node
- [ ] Design mechanism for triggering actor spawn on input availability
- [ ] Determine how to maintain maximum concurrency

**Key files to examine:**
- `ailetos/src/environment.rs` - current spawning logic
- `ailetos/src/system_runtime.rs` - I/O request handling
- `ailetos/src/scheduler.rs` - node ordering

### Phase 3: Implementation

TBD - design from Phase 2 will inform this.

### Phase 4: Testing with dbg actor

- [ ] Create DAG with multiple `dbg` instances
- [ ] Verify on-demand spawning behavior
- [ ] Verify maximum concurrency
- [ ] Test pause/resume from dagsh

## Context for Resuming Work

Current spawning (Phase 1 analysis done):
- `Environment::spawn_actor_tasks` spawns all actors upfront in topological order
- Uses `Scheduler::iter()` to get all nodes that need to run
- Creates `BlockingActorRuntime` and spawns task immediately
- All actors start before any I/O happens

To implement on-demand spawning, need to:
1. Defer actor spawning until first input arrives
2. Detect when input becomes available (SystemRuntime sees writes to dependencies)
3. Spawn actor task at that moment
4. Maintain maximum concurrency (spawn as many as have input ready)
