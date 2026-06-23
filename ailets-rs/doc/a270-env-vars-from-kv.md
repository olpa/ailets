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
An in-process store behind a `RwLock`. The CLI populates it via
`EnvService::set(key, value)` before launching actors. Actors reach it
through `ActorRuntime::get_env`. **`VarStore` is kept** — it is the source
of truth for CLI-supplied overrides.

`VarStore` supports per-actor variables: each entry carries an optional actor
id (`Option<u32>`, where `None` means global / pid 0). Storage is a
`Vec<(Option<u32>, String, String)>` with linear search — the list is short
(CLI flags only) so a `HashMap` is unnecessary overhead. Lookup checks
per-actor entries first, then falls back to global entries.

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
| `cli/src/lib.rs:222` | `env_service.set(key, value)` — the write side (becomes `set(None, key, value)`) |
| `ailetos/tests/actor_syscall/blocking_actor_runtime.rs` | passes `Arc<EnvService>` into `BlockingActorRuntime::new` |

## Target design

### KV path convention

```
/env/0/<var_name>       # global default (pid 0)
/env/<pid>/<var_name>   # per-actor override
```

`pid` is the actor's `node_handle` integer (`Handle::id()`).

### Virtual KV wrapper (`VarKV`)

Lives at `ailetos/src/storage/varkv.rs` alongside the other storage backends.

A new struct that wraps an inner `Arc<dyn KVBuffers>` and implements
`KVBuffers`. For any path that starts with `/env/`:

- `open("…", Read)`: parse `/<pid>/<key>` from the remainder, look up `key`
  in `VarStore`, fall back to `std::env::var(key)`. Return a synthesised
  completed buffer on success, `KVError::NotFound` if absent.
- `open("…", Write)`: return `KVError::Backend("read-only")`.
- `listdir("/env/<pid>/")`: return the union of keys present in `VarStore`
  and `std::env::vars()`. This is required to support callers that iterate
  over all variables with a given prefix (see `messages_to_query` below).
- `stat`: return `KVError::NotFound` — not needed.

For all other paths: delegate to inner KV unchanged.

The CLI calls `var_store.set(None, key, value)` for global variables and
`var_store.set(Some(pid), key, value)` for per-actor overrides.

```rust
// ailetos/src/storage/varkv.rs  (new file)
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

`VarStore` (renamed from `EnvService`) gains per-actor support. The internal
`HashMap<String, String>` is replaced with a `Vec<(Option<u32>, String, String)>`
where `None` means global (pid 0) and `Some(pid)` means per-actor. Lookup does
a linear scan: per-actor match first, then global fallback. The CLI calls
`var_store.set(None, key, value)` for global vars (same semantics as before).
`VarStore` is no longer handed to `BlockingActorRuntime` directly; instead it
lives inside `VarKV` and is consulted only when a KV open on an `/env/` path
is requested.

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

### `messages_to_query` — `EnvOpts` deletion

`messages_to_query` currently uses two mechanisms that both go away:

**1. `StdHandle::Env` fd** (`messages_to_query/src/lib.rs:210`)
A JSON blob is read from fd 3 into `EnvOpts` at actor startup. This fd and
the `EnvOpts` struct are both deleted in this refactor.

**2. `EnvOpts` usage in `StructureBuilder`** (`structure_builder.rs`)

`EnvOpts` is used in two ways that must both be preserved after deletion:

*Named key lookups* — straightforward, replace each with `get_env(runtime, kv, key)`:
- `env_opts.get("http.header.Content-type")`
- `env_opts.get("http.header.Authorization")`
- `env_opts.get("llm.stream")`

*Prefix iteration* — the tricky part. Two loops iterate over all keys with a
given prefix and forward them verbatim to the LLM request:
```rust
// loop 1: extra HTTP headers
for (key, value) in &self.env_opts {
    if key.starts_with("http.header.") && key != "http.header.Content-type" … { … }
}
// loop 2: extra LLM body params
for (key, value) in &self.env_opts {
    if key.starts_with("llm.") && key != "llm.model" … { … }
}
```
Replace each loop with: call `kv.listdir("/env/<pid>/")`, filter keys by
prefix, then `get_env(runtime, kv, key)` for each.

After both replacements, delete `messages_to_query/src/env_opts.rs`, remove
the `env_opts` field from `StructureBuilder`, and replace the `EnvOpts`
constructor parameter with `kv: &dyn KVBuffers`.

## Step-by-step for a developer

1. **Rename `EnvService` → `VarStore`** (`ailetos/src/env_service.rs` →
   `ailetos/src/var_store.rs`). Update the module declaration in `lib.rs` and
   all import sites. Change internal storage from `HashMap<String, String>` to
   `Vec<(Option<u32>, String, String)>`. Update `set(key, value)` →
   `set(pid: Option<u32>, key, value)` and `get(key)` → `get(pid: u32, key)`
   (linear scan: per-actor match first, then `None`/global fallback). Add a
   `keys(pid: u32)` method returning the union of per-actor and global keys
   (needed by `VarKV::listdir`).

2. **Add `VarKV`** (`ailetos/src/storage/varkv.rs`):
   - Implement `KVBuffers` wrapping inner KV.
   - `open("/env/…", Read)`: parse pid and key, look up `VarStore` then
     `std::env::var`, synthesise a buffer or return `NotFound`.
   - `listdir("/env/<pid>/")`: return union of `VarStore` keys and
     `std::env::vars()` keys (needed for prefix iteration in `messages_to_query`).
   - All other paths: delegate to inner.
   - Register in `ailetos/src/storage/mod.rs`.
   - Wire into `Environment::new`: wrap the incoming `kv` before storing it
     (`self.kv = Arc::new(VarKV::new(kv, Arc::clone(&var_store)))`).

3. **Add `get_env` free function** (`actor_io/src/env.rs`).
   Check `ailetos/src/storage/buffer.rs` for the correct method to read buffer
   contents.

4. **Remove `ActorRuntime::get_env`** from the trait and all impls. Run
   `cargo check` to find remaining usages.

5. **Delete `EnvOpts` and update `messages_to_query`** — see the
   `messages_to_query` section above for the full breakdown:
   - Replace named key lookups with `get_env(runtime, kv, key)`.
   - Replace prefix-iteration loops with `kv.listdir` + filter + `get_env`.
   - Remove `StdHandle::Env` fd read from `lib.rs:210`.
   - Delete `env_opts.rs`; remove `env_opts` field and constructor parameter
     from `StructureBuilder`; add `kv: &dyn KVBuffers` parameter instead.

6. **Remove `var_store` from `BlockingActorRuntime`**: remove the field and
   the constructor parameter. Update call sites in
   `ailetos/tests/actor_syscall/blocking_actor_runtime.rs`.

7. **Remove `StdHandle::Env`** (fd 3). Renumber `Metrics` to 3 and `Trace` to 4,
   update `_Count`. Remove the `"env"` arm in `TryFrom<&str>`.
   Remove the `register_std_fd_reader` call for it in `BlockingActorRuntime`.
   Fix the pool test at `ailetos/tests/pipe/pool.rs:120,134`.

8. **Run `cargo test --workspace`** and fix remaining failures.

## Files to touch (summary)

```
ailetos/src/env_service.rs            # rename to var_store.rs; rename EnvService → VarStore inside
ailetos/src/storage/varkv.rs          # new — VarKV wrapper
ailetos/src/storage/mod.rs            # register varkv module
ailetos/src/lib.rs                    # rename env_service module to var_store; keep re-export
ailetos/src/environment.rs            # wrap kv in VarKV at construction
ailetos/src/actor_syscall/blocking_actor_runtime.rs
                                      # remove var_store field+param, get_env impl, Env fd registration
ailetos/tests/actor_syscall/blocking_actor_runtime.rs
                                      # remove Arc<VarStore> constructor args
ailetos/tests/pipe/pool.rs            # fix StdHandle::Env references
actor_runtime/src/lib.rs              # remove StdHandle::Env, renumber
actor_runtime/src/runtime_trait.rs    # remove get_env method
actor_runtime/src/ffi_runtime.rs      # remove get_env impl
actor_runtime_mocked/src/vfs.rs       # remove env_vars field, get_env impl
messages_to_query/src/env_opts.rs     # delete
messages_to_query/src/structure_builder.rs
                                      # replace get_env + EnvOpts iteration with free fn + listdir
messages_to_query/src/lib.rs          # remove StdHandle::Env fd read and EnvOpts construction
actor_io/src/env.rs                   # new — get_env free function
```

## Work plan

- [ ] Rename `EnvService` → `VarStore`
- [ ] Add `VarKV` (`ailetos/src/storage/varkv.rs`)
- [ ] Add `get_env` free function (`actor_io/src/env.rs`)
- [ ] Remove `ActorRuntime::get_env` from trait and all impls
- [ ] Delete `EnvOpts`; update `messages_to_query` (named lookups + prefix iteration)
- [ ] Remove `var_store` from `BlockingActorRuntime`
- [ ] Remove `StdHandle::Env` (fd 3)
- [ ] `cargo test --workspace` green
