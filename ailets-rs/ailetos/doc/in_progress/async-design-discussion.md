# Async Design Discussion

**Status:** Points for discussion. Not a plan.

## Context

A developer working on pipe attachment made `Environment::attach_stdout_to` async, which
cascaded through `cmd_run`, `cmd_follow`, `execute`, and tests before the work was left
incomplete. The changes were reverted. This document records the architectural analysis.

The underlying goal was legitimate: `attach_stdout_to` needed to work for pipes that are
already realized (e.g. value nodes) at the time of registration. The pipe pool race and the
value-node bypass are tracked separately in `pipe-pool-notification-race.md`.

## Points for Discussion

**1. Should `ailetos` call `tokio::spawn` as a global function in public APIs?**

Established during analysis: No.

`tokio::spawn` uses `Handle::current()` implicitly — it spawns on whatever runtime is active
at the call site. `ailetos` has a dedicated `ailetos_rt`. If `attach_stdout_to` is called from
a different runtime (e.g. a test runtime), tasks spawn there while `PipePool`'s `Notify`
objects were created on `ailetos_rt`. Tokio sync primitives are runtime-bound; cross-runtime
use silently misbehaves.

The executor and `AttachmentManager` are the correct spawn owners: they run on `ailetos_rt`
and the runtime context is guaranteed. Public APIs should register intent (sync), not spawn.

**2. Should `DagShell::execute` ever become async?**

Established during analysis: the sync boundary is correct for now.

`execute` is a REPL dispatcher. Its callers (readline loop, tests) are sync. The
`ailetos_rt.block_on(cmd_run(...))` pattern is the right bridge. Making `execute` async would
couple callers to tokio, require `#[tokio::test]` everywhere, and risk cross-runtime primitive
use between the caller's runtime and `ailetos_rt`.

Open question: the `self.ailetos_rt.block_on(self.cmd_run(...))` idiom fails the borrow
checker (double borrow of `self`). The fix is `self.ailetos_rt.handle().clone().block_on(...)`.
Is this the right long-term idiom, or a sign the struct should be split?

**3. Two attachment mechanisms coexist**

- `AttachmentConfig` + `AttachmentManager`: event-driven, spawns when writer is realized,
  sync registration, owns `JoinHandle`s, has `shutdown()`. Architecturally correct.
- `Environment::attach_stdout_to`: was a convenience wrapper, was changed to spawn directly,
  reverted back to delegating to `AttachmentConfig`.

After fixing the pipe pool race (option A: `notify_one`) and the value-node bypass, does
`AttachmentConfig` correctly handle all cases? If yes, `attach_stdout_to` remains a thin sync
wrapper and no second mechanism is needed.

Question: should `AttachmentManager::on_writer_realized` also handle already-realized pipes
(i.e. be callable after the writer exists)? Currently it is triggered only at realization time.

**4. The clean call chain**

The current intended flow — worth preserving explicitly:

```
CLI sync code
  └─ block_on(ailetos_rt) [ enter ailetos runtime ]
       └─ attachment_config.attach_to_sink(handle, sink)  [ sync registration ]
       └─ executor.submit(handle, ...)
            └─ actor runs, touch_writer() called
                 └─ AttachmentManager.on_writer_realized()  [ spawns on ailetos_rt, correct ]
                      └─ get_or_await_new_reader + copy_to_writer  [ async, on ailetos_rt ]
```

Async machinery stays entirely inside `ailetos_rt`. The CLI never participates in it.

**5. If `execute` were made async in the future (not recommended now)**

For completeness, what that migration would require:
- Remove owned `ailetos_rt` from `DagShell` (or keep it only for executor startup)
- Tests migrate from `#[test]` + direct calls to `#[tokio::test]`
- `main.rs` becomes `#[tokio::main]`, rustyline calls wrapped in `spawn_blocking`
- All internal `block_on` calls removed from `execute`
- Verify no cross-runtime tokio primitive use between caller runtime and executor tasks

This is a deliberate migration, not something to drift into. It should be decided as a
milestone, not triggered by a single feature need.
