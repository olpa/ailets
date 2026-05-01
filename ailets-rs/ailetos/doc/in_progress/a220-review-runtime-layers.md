# Runtime Layer Refactoring

**Branch**: a220-review-runtime-layers

---

## Phase 1 — IoBridge Extraction ✓ COMPLETE

Original goal: extract `SystemRuntime` into a clean `IoBridge` by removing three foreign responsibilities (DAG state in ActorShutdown, DAG read in materialize_stdin, spawn_notify ownership) and renaming the struct. All four steps completed.

Additional cleanup done in this branch:
- Switch `actor_syscall` to `parking_lot::Mutex`
- Replace `let _ = send(...)` with `warn!` on dropped receivers
- Update module docs to reflect direct-call model
- Document `pipe_pool` ownership intent (`regression:` comment)

---

## Phase 2 — Environment / RunHandle Merge & Live DAG Preparation

### Background

`Environment` (build phase) and `RunHandle` (run phase) are structurally identical — six fields, same types. `make_run_handle()` clones two fields (`actor_registry`, `attachment_config`) and Arc-clones the other four. The clone/snapshot semantics are wrong: all six fields should be Arc-shared so executor always sees current state. This is not just cleanup — it is architecturally required for live DAG support (see Step 5).

Future capability: nodes will be addable during execution ("live DAG"). The steps below prepare the architecture for that without implementing it yet.

---

### Step 1 — Arc-share `actor_registry` and `attachment_config`

**Goal**: Eliminate snapshot semantics. Both fields must be Arc-shared so the executor always sees the current state, including registrations made after a run starts.

- `actor_registry: Arc<RwLock<ActorRegistry>>`
- `attachment_config: Arc<AttachmentConfig>` (RwLock if runtime mutation is needed)
- Update build-phase mutation methods to acquire write lock
- Update executor read sites to acquire read lock (or snapshot via `read().clone()` at spawn time if contention is a concern)

---

### Step 2 — Merge `Environment` and `RunHandle`

**Goal**: One type. `RunHandle` is deleted; `Environment` serves as the single system handle for both build and run phases.

- Make `attachment_config` `pub(crate)` (executor access)
- Implement `Clone` for `Environment` — pure Arc::clone of all six fields, zero heap allocation
- Change `executor::run` and `run_with_tx` to take `Arc<Environment>` instead of `&RunHandle`
- `Environment::run()` becomes `executor::run(Arc::new(self.clone()), ...)`
- Delete `RunHandle` and `make_run_handle()`
- Update tests and examples

**Dependency**: Step 1 must be complete.

---

### Step 3 — Move `PipePool` to `Environment`

**Goal**: `PipePool` is the runtime realization of DAG edges — it conceptually belongs to the system runtime, not to `IoBridge` (see `regression:` comment in `IoBridge::new`).

- Add `pipe_pool: Arc<PipePool>` field to `Environment` (created in `Environment::new()` from `kv`)
- Remove `pipe_pool` parameter from `IoBridge::new`
- `is_ready_to_spawn` receives `&Environment` instead of `&PipePool` (or accesses it through the env)
- Remove the local `pipe_pool` variable in `run_with_tx`

**Dependency**: Step 2 must be complete.

---

### Step 4 — `IoBridge` holds `Arc<Environment>`

**Goal**: `IoBridge` accesses all shared state through the merged environment instead of individually extracted fields. This is the access path required for future actor-initiated node creation.

- Remove `kv`, `id_gen`, `attachment_manager`, `pipe_pool` fields from `IoBridge`
- `IoBridge` stores `Arc<Environment>`, accesses fields through it
- `IoBridge::new` signature shrinks to `(env: Arc<Environment>, notify: Arc<Notify>, actor_done_tx: ...)`
- Internal method bodies updated to use `self.env.kv`, `self.env.id_gen`, etc.

**Dependency**: Step 3 must be complete.

---

### Step 5 — Structural preparation for live DAG

**Goal**: Make the executor structurally ready to accept nodes added during execution. No live-DAG functionality yet — only the structural changes that would otherwise require a disruptive rewrite later.

- **Replace static `pending` list with per-iteration re-scan**: currently computed once at startup; change the spawn loop to scan all `NotStarted` nodes each iteration (small change, non-breaking, unlocks dynamic node pickup)
- **Document `WorkSource` quiescence model**: the executor needs a "no more nodes will ever arrive" signal to know when to stop. Design: a `WorkSource` handle (like `actor_done_tx`) held by anything that can add nodes; when all are dropped, the executor may exit after draining. No implementation yet — document the concept and the intended integration point.
- **Reserve `spawn_node` in `ActorRuntime` trait**: add the method signature with a `todo!()` / `unimplemented!()` body so the trait boundary is established and actor implementations know what's coming.

**Dependency**: Step 4 must be complete (`IoBridge` holding `Arc<Environment>` is required for `spawn_node` to have a path to the DAG).
