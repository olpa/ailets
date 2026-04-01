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
- Create `dbg_control` module in CLI (not in ailetos - keeps ailetos clean)
- Use thread-local storage + static global registry
- Registry: `HashMap<Handle, Arc<DbgControl>>`
- State: `enum DbgControlState { Running, Paused }`
- Control: `struct DbgControl { state: Mutex<DbgControlState>, condvar: Condvar }`
- API: `register_dbg_actor(handle)`, `resume_dbg_actor(handle)`, `init_dbg_actor(handle)`

**2. Actor Structure** (`cli/src/dbg_actor.rs` and `cli/src/dbg_control.rs`)
- Inlined into CLI as normal modules (not a separate crate)
- `dbg_actor.rs`:
  - Get byte limit from thread-local (set by init hook, default 100)
  - Pass through N bytes from stdin to stdout
  - Enter paused state (wait on condvar in control registry)
  - On resume: continue copying until EOF
  - Log start/finish with actor handle via tracing
- `dbg_control.rs`:
  - Global registry + thread-local storage for control handles
  - `init_dbg_actor(handle)` - called before running DAG

**3. Integration with CLI**
- Add `dbg_actor` and `dbg_control` modules to `cli/src/`
- Update `cli/Cargo.toml` to add dependencies: `embedded-io`, `tracing`
- Register actor in `cli/src/main.rs`: `env.actor_registry.register("dbg", dbg_actor::execute)`
- Before running the DAG, initialize debug actors by checking `idname`:
  - Iterate through all nodes in the DAG
  - For nodes with `idname == "dbg"`, call `dbg_control::init_dbg_actor(node_handle)`
- Add `resume <node>` command that calls `dbg_control::resume_dbg_actor(handle)`

**Files created/modified:**
- `cli/src/dbg_actor.rs` - ✅ created (debug actor implementation, refactored to use runtime.node_handle())
- `cli/src/dbg_control.rs` - ✅ created (control registry, refactored to remove thread-local storage)
- `cli/src/main.rs` - ✅ modified (add modules, register dbg actor, initialize by idname, add resume command)
- `cli/Cargo.toml` - ✅ modified (add embedded-io, tracing, actor_runtime, and once_cell dependencies)
- `ailetos/src/environment.rs` - ✅ modified (reverted metadata system, simple ActorRegistry, updated ActorFn signature)
- `ailetos/src/stub_actor_runtime.rs` - ✅ modified (added node_handle() getter method)
- `ailetos/src/lib.rs` - ✅ modified (simplified exports)
- `Cargo.toml` - ✅ modified (removed dbg from workspace members)
- `cli/scripts/test_dbg.dagsh` - ✅ created (test script)
- `cat/src/lib.rs` - ✅ modified (updated to new ActorFn signature with helper function for WASM)
- `cat/Cargo.toml` - ✅ modified (added ailetos dependency)

**Phase 1 Complete!**

Implementation notes:
- The `dbg` actor is inlined into CLI as normal modules (`dbg_actor.rs`, `dbg_control.rs`)
- Not a separate crate - simplifies the architecture
- Configuration is passed via thread-local storage (byte_limit) set by the init hook
- Control commands are sent from dagsh via `resume <node>` command
- The actor uses tracing for logging with the node handle
- Test script: `cli/scripts/test_dbg.dagsh`

**Actor Signature Change (Critical Fix):**
- Changed `ActorFn` from `fn(AReader, AWriter) -> Result<(), String>` to `fn(BlockingActorRuntime) -> Result<(), String>`
- Actors now receive the full runtime instead of just pre-created reader/writer
- This allows actors to:
  - Access stderr for logging
  - Open additional file descriptors beyond stdin/stdout
  - Get their node handle for identification via `runtime.node_handle()`
  - Control runtime behavior (e.g., pause/resume in dbg actor)
- Updated `spawn_actor_task` to pass runtime to actor function
- Both `cat` and `dbg` actors updated to new signature
- WASM compatibility maintained via helper function pattern in `cat` actor
- Added `node_handle()` getter method to `BlockingActorRuntime`

**Dbg Control Refactoring (Removed Thread-Local Storage):**
- Refactored `dbg_control.rs` to eliminate thread-local storage
- Now uses only a global registry: `HashMap<Handle, Arc<DbgControl>>`
- Actors look up their control structure by their node handle
- Simplified architecture:
  - Actor calls `runtime.node_handle()` to get its ID
  - Looks up control via `dbg_control::get_dbg_control(handle)`
  - No more thread-local setup before spawning
- DbgControl now stores optional byte_limit configuration
- Cleaner, more direct access pattern using actor IDs

**Architecture:**
- `ActorRegistry` remains simple: maps actor name to actor function only
- Debug actor is inlined into CLI as normal modules, not a separate crate
- Debug actor initialization is handled in CLI layer, not in ailetos
- Before running the DAG, CLI iterates nodes and checks `idname` field
- For nodes with `idname == "dbg"`, CLI calls `dbg_control::init_dbg_actor(handle)`
- This approach uses the existing `idname` field to differentiate actors, avoiding metadata complexity
- Keeps ailetos clean and free from dependencies on specific actor implementations
- Actor functions receive `BlockingActorRuntime` giving them full access to runtime capabilities

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

