# Testing actors in ailets-rs

## The split: `execute` vs `execute_impl`

The standard actor structure separates runtime wiring from business logic:

```rust
// Business logic: takes plain reader/writer, no runtime knowledge
fn execute_impl(mut reader: impl embedded_io::Read, mut writer: impl embedded_io::Write) -> Result<(), String> {
    // ...
}

// Entry point: wires up I/O from runtime, then delegates
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let reader = AReader::new_from_std(runtime, StdHandle::Stdin);
    let writer = AWriter::new_from_std(runtime, StdHandle::Stdout);
    execute_impl(reader, writer)
}
```

The payoff is in tests: `execute_impl` accepts any `embedded_io::Read`/`Write`, so tests pass `&[u8]` and `RcWriter` directly — no runtime setup needed. The `execute` function is thin enough that it doesn't need its own test.

See `cat/src/lib.rs` for a minimal example.

## Two patterns

Actors expose two kinds of entry points, and the right test approach depends on which one you're working with.

### Pattern A — injected reader/writer

Actors that follow the split above expose a `execute_impl` / `process_*` function that takes explicit reader and writer arguments. These are straightforward to test.

```rust
// reader: any &[u8] (implements embedded_io::Read via the blanket impl)
// writer: RcWriter from actor_runtime_mocked
use actor_runtime_mocked::RcWriter;

let writer = RcWriter::new();
my_crate::execute_impl(input_bytes, writer.clone())?;
assert_eq!(writer.get_output(), "expected output");
```

`RcWriter` implements `embedded_io::Write` and is cheaply cloned (shared `Rc<RefCell<Vec<u8>>>`). Call `.get_output()` after the actor finishes to get the collected bytes as a `String`.

See `messages_to_markdown/tests/actor_test.rs` and `messages_to_query/tests/actor_test.rs` for examples.

### Pattern B — ActorRuntime entry point

Some actors only expose `execute(runtime: &dyn ActorRuntime)`. These read from `StdHandle::Stdin` (fd 0) and write to `StdHandle::Stdout` (fd 1).

Use `VfsActorRuntime` from `actor_runtime_mocked`. The VFS assigns fds in the order handles are opened, so you must open them before calling `execute`:

```rust
use actor_runtime::ActorRuntime; // needed to call open_read / open_write
use actor_runtime_mocked::VfsActorRuntime;

let runtime = VfsActorRuntime::new();
runtime.add_file("stdin".to_string(), input_bytes.to_vec());
let _ = runtime.open_read("stdin");   // → handle 0 = stdin
let _ = runtime.open_write("stdout"); // → handle 1 = stdout

my_crate::execute(&runtime)?;

let output = runtime.get_file("stdout").unwrap();
```

**Gotcha — VFS and newlines:** `VfsActorRuntime::awrite` stops at `'\n'` (the `IO_INTERRUPT` sentinel) and returns. `embedded_io::write_all` retries until all bytes are written, so multi-line output is fine; just be aware if you hit unexpected partial-write behaviour.

## Mocking external dependencies

Some actors need additional dependency injection beyond I/O.

| Dependency | Approach | Example |
|-----------|----------|---------|
| DAG operations | Implement a mock struct for the `DagOps` trait | `gpt/tests/dagops_mock.rs` |
| HTTP (ureq) | Pass a `ureq::Agent` built with `Agent::with_parts` and a `FakeConnector` | `query/tests/execute_test.rs` |
| Environment variables | Inject a `&dyn Fn(&str) -> Option<String>` closure | `query/tests/resolve_secrets_test.rs` |

### ureq HTTP mocking

For actors that make HTTP calls, implement `Connector` + `Transport` from `ureq::unversioned::transport` and build an agent in the test. The actor exposes `execute_impl(reader, writer, agent)` alongside the production `execute(runtime)`:

```rust
use actor_runtime_mocked::RcWriter;
use ureq::unversioned::transport::{Buffers, ConnectionDetails, Connector, LazyBuffers, NextTimeout, Transport};
use ureq::unversioned::resolver::DefaultResolver;

// FakeTransport feeds a hardcoded raw HTTP response to ureq
// FakeConnector creates FakeTransport instances
// See query/tests/execute_test.rs for the full implementation (~50 lines)

let agent = ureq::Agent::with_parts(
    ureq::config::Config::default(),
    FakeConnector { response: b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello".to_vec() },
    DefaultResolver::default(),
);
let writer = RcWriter::new();
my_crate::execute_impl(input_bytes, writer.clone(), &agent)?;
assert_eq!(writer.get_output(), "hello");
```

## Dev-dependencies

Add to `[dev-dependencies]` in `Cargo.toml`:

```toml
actor_runtime_mocked.workspace = true  # VfsActorRuntime, RcWriter
```

`actor_runtime` (for the `ActorRuntime` trait) is typically already a regular dependency.
