# A270 — Env vars from KV

Branch: `a270-env-vars-from-kv`

## Goal

Replace `ActorRuntime::get_env` with a free function `get_env(runtime, kv, key)`
that reads from virtual KV paths `/env/<pid>/<var_name>` (per-actor) and
`/env/0/<var_name>` (global default, pid 0 = "no specific actor").

The paths are **virtual**: a new `EnvKV` wrapper intercepts them and synthesises
values on-the-fly from `EnvService` (CLI overrides) and OS env vars. No bytes
are ever written to backing storage for these paths.

## Naming

`EnvService` is renamed to **`VarStore`** and the new KV wrapper is called
**`VarKV`**. Both names avoid collision with the existing `Environment` class.
The name "Variables" was considered (Airflow, Prefect, and GitHub Actions all
use that term for the same concept), but `VarStore` is more idiomatic Rust for
a struct that holds a map of values.

## Current state

### `EnvService` → `VarStore` (`ailetos/src/env_service.rs`)
An in-process `HashMap<String, String>` behind a `RwLock`. The CLI populates
it via `EnvService::set(key, value)` before launching actors. Actors reach it
through `ActorRuntime::get_env`. **`VarStore` is kept** — it is the source
of truth for CLI-supplied overrides.

### `ActorRuntime::get_env` (`actor_runtime/src/runtime_trait.rs:47`)
A method on the `ActorRuntime` trait:
```rust
fn get_env(&self, key: &str) -> Option<String>;
```
Implemented in:
- `BlockingActorRuntime` (`ailetos/src/actor_syscall/blocking_actor_runtime.rs:238`) — delegates to `env_service.get(key)`.
- `FfiActorRuntime` (`actor_runtime/src/ffi_runtime.rs:97`) — always returns `None`.
- `MockActorRuntime` (`actor_runtime_mocked/src/vfs.rs:353`) — reads from an injected `HashMap`.

### `StdHandle::Env` (fd 3) (`actor_runtime/src/lib.rs:22`)
A dedicated file-descriptor slot reserved for env data:
```rust
pub enum StdHandle { Stdin=0, Stdout=1, Log=2, Env=3, Metrics=4, Trace=5, _Count }
```
`BlockingActorRuntime::register_std_fds` registers fd 3 as a reader
(`ailetos/src/actor_syscall/blocking_actor_runtime.rs:180`).
`messages_to_query` reads its configuration from this fd
(`messages_to_query/src/lib.rs:210`):
```rust
let env_reader = AReader::new_from_std(&runtime, StdHandle::Env);
let env_opts = EnvOpts::envopts_from_reader(env_reader)?;
```

### Callers of `get_env`
| file | keys read |
|------|-----------|
| `messages_to_query/src/structure_builder.rs` | `AILETS_LLM_URL`, `AILETS_MODEL`, `AILETS_LLM_STREAM`, `AILETS_LLM_THINKING` |
| `cli/src/lib.rs:222` | `env_service.set(key, value)` — the write side |
| `ailetos/tests/actor_syscall/blocking_actor_runtime.rs` | passes `Arc<EnvService>` into `BlockingActorRuntime::new` |

## Target design

### KV path convention

```
/env/0/<var_name>       # global default (pid 0)
/env/<pid>/<var_name>   # per-actor override
```

`pid` is the actor's `node_handle` integer (`Handle::id()`).

### Virtual KV wrapper (`VarKV`)

A new struct that wraps an inner `Arc<dyn KVBuffers>` and implements
`KVBuffers`. For any path that starts with `/env/`:

1. Parse `/<pid>/<key>` from the remainder.
2. If `pid != 0`: look up `key` in `VarStore`. If found, return a
   synthesised completed buffer containing the UTF-8 value.
3. Fall back to OS env via `std::env::var(key)`. If found, same.
4. Return `KVError::NotFound` if neither source has the key.

For all other paths: delegate to the inner KV unchanged.

`listdir("/env/")` and `stat("/env/…")` can return `KVError::NotFound` for
now — these are not needed by `get_env`.

`VarKV` does **not** implement `open(…, Write)` for `/env/` paths; attempts
return `KVError::Backend("read-only")`. The CLI continues to call
`var_store.set(key, value)` unchanged.

```rust
// ailetos/src/var_kv.rs  (new file)
pub struct VarKV {
    inner: Arc<dyn KVBuffers>,
    var_store: Arc<VarStore>,
}

impl VarKV {
    pub fn new(inner: Arc<dyn KVBuffers>, var_store: Arc<VarStore>) -> Self {
        Self { inner, var_store }
    }
}
```

`Environment` wraps its KV in `VarKV` at construction time. From that point
all KV access — pipes, env vars — goes through the single `Arc<dyn KVBuffers>`
stored in `Environment`. No router needed.

### New free function `get_env`

Add a module (e.g. `actor_io/src/env.rs` or a new tiny crate) that exposes:

```rust
pub fn get_env(runtime: &dyn ActorRuntime, kv: &dyn KVBuffers, key: &str) -> Option<String> {
    let pid = runtime.node_handle();
    try_read(kv, &format!("/env/{}/{}", pid, key))
        .or_else(|| try_read(kv, &format!("/env/0/{}", key)))
}

fn try_read(kv: &dyn KVBuffers, path: &str) -> Option<String> {
    let handle = tokio::runtime::Handle::current();
    handle.block_on(async {
        let buf = kv.open(path, OpenMode::Read).await.ok()?;
        // read buf contents — check Buffer API in ailetos/src/storage/buffer.rs
        let bytes: Vec<u8> = /* buf.read_all() or similar */;
        String::from_utf8(bytes).ok()
    })
}
```

Because `KVBuffers` is `async` and actors run synchronously,
`Handle::current().block_on(…)` is the bridge — same pattern already used
elsewhere in `BlockingActorRuntime`.

### What changes with `VarStore`

`VarStore` (renamed from `EnvService`) is **kept as-is**. The CLI still calls
`var_store.set(key, value)`. The difference is that `VarStore` is no longer
handed to `BlockingActorRuntime` directly; instead it lives inside `VarKV` and
is consulted only when a KV open on an `/env/` path is requested.

## What to delete

| item | location | reason |
|------|----------|--------|
| `env_service` field on `BlockingActorRuntime` | `ailetos/src/actor_syscall/blocking_actor_runtime.rs:42` | now inside VarKV |
| `Arc<EnvService>` param in `BlockingActorRuntime::new` | same file, line 69 | |
| `ActorRuntime::get_env` method | `actor_runtime/src/runtime_trait.rs:47` | replaced by free fn |
| `StdHandle::Env` variant (fd 3) | `actor_runtime/src/lib.rs:22` | no longer a std fd |
| `register_std_fd_reader` for `StdHandle::Env` | `ailetos/src/actor_syscall/blocking_actor_runtime.rs:180` | |
| `"env"` arm in `TryFrom<&str> for StdHandle` | `actor_runtime/src/lib.rs:37` | |
| `get_env` impl in `FfiActorRuntime` | `actor_runtime/src/ffi_runtime.rs:97` | |
| `get_env` impl in `MockActorRuntime` | `actor_runtime_mocked/src/vfs.rs:353` | |
| `env_vars` field in `MockActorRuntime` | `actor_runtime_mocked/src/vfs.rs` | |

### `messages_to_query` env fd

`messages_to_query/src/lib.rs:210` reads a JSON config blob from
`StdHandle::Env`. This is a separate mechanism from the named-key `get_env`
calls in `structure_builder.rs` and is **not** part of this refactor.
Rename the variant to `StdHandle::Config` to avoid confusion with the deleted
semantics, and note a follow-up ticket to decide whether to port it to KV too.

## Step-by-step for a developer

1. **Rename `EnvService` → `VarStore`** (`ailetos/src/env_service.rs` →
   `ailetos/src/var_store.rs`). Update the module declaration in `lib.rs` and
   all import sites. The struct API (`new`, `set`, `get`) is otherwise unchanged.

2. **Add `VarKV`** (`ailetos/src/var_kv.rs`):
   - Implement `KVBuffers` wrapping inner KV.
   - On `open("/env/…", Read)`: parse pid and key, look up `VarStore` then
     `std::env::var`, synthesise a buffer or return `NotFound`.
   - All other paths: delegate to inner.
   - Wire `VarKV` into `Environment::new`: wrap the incoming `kv` before
     storing it (`self.kv = Arc::new(VarKV::new(kv, Arc::clone(&var_store)))`).

3. **Add `get_env` free function** (module in `actor_io` or new crate).
   Check `ailetos/src/storage/buffer.rs` for the correct method to read buffer
   contents.

4. **Remove `ActorRuntime::get_env`** from the trait and all impls. Run
   `cargo check` to find remaining usages.

5. **Update callers** in `messages_to_query/src/structure_builder.rs`:
   replace `self.runtime.get_env(key)` with `get_env(self.runtime, kv, key)`.
   Thread `kv: &dyn KVBuffers` through `StructureBuilder` if not already present.

6. **Remove `var_store` from `BlockingActorRuntime`**: remove the field and
   the constructor parameter. Update call sites in
   `ailetos/tests/actor_syscall/blocking_actor_runtime.rs`.

7. **Remove `StdHandle::Env`** (fd 3). Renumber `Metrics` to 3 and `Trace` to 4,
   update `_Count`. Remove the `"env"` arm in `TryFrom<&str>`.
   If keeping the env fd for `messages_to_query`, add `Config = 3` first, then
   remove `Env`. Fix the pool test at `ailetos/tests/pipe/pool.rs:120,134`.

8. **Run `cargo test --workspace`** and fix remaining failures.

## Files to touch (summary)

```
ailetos/src/env_service.rs            # rename to var_store.rs; rename EnvService → VarStore inside
ailetos/src/var_kv.rs                 # new — VarKV wrapper
ailetos/src/lib.rs                    # rename env_service module to var_store; add var_kv module
ailetos/src/environment.rs            # wrap kv in VarKV at construction
ailetos/src/actor_syscall/blocking_actor_runtime.rs
                                      # remove var_store field+param, get_env impl, Env fd registration
ailetos/tests/actor_syscall/blocking_actor_runtime.rs
                                      # remove Arc<VarStore> constructor args
ailetos/tests/pipe/pool.rs            # fix StdHandle::Env references
actor_runtime/src/lib.rs              # remove StdHandle::Env, renumber; add Config if needed
actor_runtime/src/runtime_trait.rs    # remove get_env method
actor_runtime/src/ffi_runtime.rs      # remove get_env impl
actor_runtime_mocked/src/vfs.rs       # remove env_vars field, get_env impl
messages_to_query/src/structure_builder.rs
                                      # replace get_env calls with free fn; add kv param
messages_to_query/src/lib.rs          # update StdHandle::Env → Config if renamed
# new file (or module in actor_io):
actor_io/src/env.rs                   # get_env free function
```
