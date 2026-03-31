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
- [x] Create `dbg` actor (dbgug cat variant)
  - Name: `dbg`
  - Args: byte limit (default 100, configurable via thread-local)
  - Behavior: pass through N bytes, then pause
  - Control: `resume <node>` command from dagsh
  - Logging: start/finish with actor ID via tracing
  - Pause: block waiting for resume command
  - Resume: continue until EOF

#### Implementation Plan for Phase 1

**1. Global Control Registry**
- Create `dbg_control` module in `ailetos` crate
- Use `lazy_static` or `once_cell` for global registry
- Registry: `HashMap<Handle, Arc<Mutex<DbgControlState>>>`
- State: `enum DbgControlState { Running, Paused(Condvar) }`
- API: `register_dbg_actor(handle)`, `resume_dbg_actor(handle)`

**2. Actor Structure** (`dbg` crate)
- New workspace member: `dbg/`
- Cargo.toml: similar to `cat`, add `parking_lot`, `serde_json`, `tracing`
- `lib.rs`:
  - Read byte limit from `Env` handle (JSON: `{"byte_limit": 100}`)
  - Pass through N bytes from stdin to stdout
  - Register self in global control registry
  - Enter paused state (wait on condvar)
  - On resume: continue copying until EOF
  - Log start/finish with actor handle via tracing

**3. Integration**
- Update workspace `Cargo.toml` to include `dbg` member
- Update `build.sh` to compile `dbg` to WASM
- Register actor in `cli/src/main.rs`: `env.actor_registry.register("dbg", dbg::execute)`

**4. dagsh Integration** (for Phase 4)
- Add command: `resume <node_id>` or `resume <node_name>`
- Command calls `ailetos::dbg_control::resume_dbg_actor(handle)`

**Files to create/modify:**
- `dbg/Cargo.toml` - ✅ created (native-only rlib)
- `dbg/src/lib.rs` - ✅ created
- `ailetos/src/dbg_control.rs` - ✅ created
- `ailetos/src/lib.rs` - ✅ export dbg_control module
- `Cargo.toml` - ✅ add dbg to workspace members
- `build.sh` - ⏭️ skipped (dbg is native-only, not WASM)
- `cli/src/main.rs` - ✅ register dbg actor
- `ailetos/src/environment.rs` - ✅ register dbg control before spawning

**Phase 1 Complete!**

Implementation notes:
- The `dbg` actor is native-only (not compiled to WASM) to allow access to the control module
- `dbg_control` module is in the `dbg` crate, not in `ailetos` (keeps ailetos clean)
- Configuration is passed via thread-local storage (byte_limit) set by the init hook
- Control commands are sent from dagsh via `resume <node>` command
- The actor uses tracing for logging with the node handle
- Test script: `test_dbg.dagsh`

**Architecture:**
- Added general-purpose actor initialization hooks to `ailetos::ActorRegistry`
- New API: `register_with_init(name, actor_fn, init_fn)` where init_fn receives the node handle
- Init hooks are called before the actor starts executing
- This allows any actor to set up actor-specific state without polluting ailetos
- The dbg actor uses this to register itself in the global control registry

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

