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
- [x] Create `dbg` actor (debug cat variant)
  - Name: `dbg`
  - Args: byte limit (default 100, configurable via thread-local)
  - Behavior: pass through N bytes, then pause
  - Control: `resume <node>` command from dagsh
  - Logging: start/finish with actor ID via tracing
  - Pause: block waiting for resume command
  - Resume: continue until EOF

#### Implementation Plan for Phase 1

**1. Global Control Registry**
- Create `control` module in `dbg` crate (not in ailetos - keeps ailetos clean)
- Use thread-local storage + static global registry
- Registry: `HashMap<Handle, Arc<DbgControl>>`
- State: `enum DbgControlState { Running, Paused }`
- Control: `struct DbgControl { state: Mutex<DbgControlState>, condvar: Condvar }`
- API: `register_dbg_actor(handle)`, `resume_dbg_actor(handle)`, `init_dbg_actor(handle)`

**2. Actor Structure** (`dbg` crate)
- New workspace member: `dbg/`
- Cargo.toml: native-only rlib, dependencies: ailetos, serde_json, tracing
- `lib.rs`:
  - Get byte limit from thread-local (set by init hook, default 100)
  - Pass through N bytes from stdin to stdout
  - Enter paused state (wait on condvar in control registry)
  - On resume: continue copying until EOF
  - Log start/finish with actor handle via tracing
- `control.rs`:
  - Global registry + thread-local storage for control handles
  - `init_dbg_actor(handle)` - called by actor init hook

**3. Integration with CLI**
- Update workspace `Cargo.toml` to include `dbg` member
- Update `cli/Cargo.toml` to depend on `dbg` crate
- Register actor in `cli/src/main.rs`: `env.actor_registry.register("dbg", dbg::execute)`
- Before running the DAG, initialize debug actors by checking `idname`:
  - Iterate through all nodes in the DAG
  - For nodes with `idname == "dbg"`, call `dbg::control::init_dbg_actor(node_handle)`
- Add `resume <node>` command that calls `dbg::control::resume_dbg_actor(handle)`

**Files created/modified:**
- `dbg/Cargo.toml` - ✅ created (native-only rlib)
- `dbg/src/lib.rs` - ✅ created (actor implementation)
- `dbg/src/control.rs` - ✅ created (control registry)
- `ailetos/src/environment.rs` - ✅ modified (reverted metadata system, simple ActorRegistry)
- `ailetos/src/lib.rs` - ✅ modified (simplified exports)
- `Cargo.toml` - ✅ modified (add dbg to workspace members)
- `cli/Cargo.toml` - ✅ modified (add dbg dependency)
- `cli/src/main.rs` - ✅ modified (register dbg actor, initialize by idname, add resume command)
- `test_dbg.dagsh` - ✅ created (test script)

**Phase 1 Complete!**

Implementation notes:
- The `dbg` actor is native-only (not compiled to WASM) to allow access to the control module
- `dbg_control` module is in the `dbg` crate, not in `ailetos` (keeps ailetos clean)
- Configuration is passed via thread-local storage (byte_limit) set by the init hook
- Control commands are sent from dagsh via `resume <node>` command
- The actor uses tracing for logging with the node handle
- Test script: `test_dbg.dagsh`

**Architecture:**
- `ActorRegistry` remains simple: maps actor name to actor function only
- Debug actor initialization is handled in CLI layer, not in ailetos
- Before running the DAG, CLI iterates nodes and checks `idname` field
- For nodes with `idname == "dbg"`, CLI calls `dbg::control::init_dbg_actor(handle)`
- This approach uses the existing `idname` field to differentiate actors, avoiding metadata complexity
- Keeps ailetos clean and free from dependencies on specific actor implementations

### Phase 2: Analysis
- [ ] Understand current spawning behavior in `Environment::spawn_actor_tasks`
- [ ] Identify how to detect when input becomes available for a node
- [ ] Design mechanism for triggering actor spawn on input availability
- [ ] Determine how to maintain maximum concurrency

### Phase 3: Implementation
- [ ] TODO

### Phase 4: Testing with dbg actor
- [ ] Create DAG with multiple `dbg` instances
- [ ] Verify on-demand spawning behavior
- [ ] Verify maximum concurrency
- [ ] Test pause/resume from dagsh

## Notes

