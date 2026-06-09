# query crate — developer orientation

## What this crate does

Reads a JSON spec from stdin, makes an HTTP request, streams the response body to stdout. It is registered as the `"query"` actor in the CLI (`cli/src/lib.rs`).

## Entry points

| Symbol | Purpose |
|--------|---------|
| `execute(runtime)` | Production entry point; creates a default ureq agent |
| `execute_with_agent(runtime, agent)` | Same but accepts an injected agent — use this in tests |
| `resolve_secrets(value, url, get_env)` | Replaces `{{secret}}` in a header value with an API key derived from the URL domain |

## Runtime / I/O model

Actors don't use `std::io` directly. All I/O goes through the `ActorRuntime` trait (`actor_runtime` crate). Standard handles are fixed fd numbers:

- fd 0 → stdin (`StdHandle::Stdin`)
- fd 1 → stdout (`StdHandle::Stdout`)

`AReader` / `AWriter` (`actor_io` crate) wrap a runtime fd and implement `embedded_io::Read` / `Write`.

## Testing

**For `resolve_secrets`** — inject a plain closure:
```rust
let get_env = |k: &str| match k { "FOO_API_KEY" => Some("token".into()), _ => None };
resolve_secrets("Bearer {{secret}}", "https://foo.example.com/v1", &get_env)
```
See `tests/resolve_secrets_test.rs`.

**For `execute_with_agent`** — two things to set up:

1. **Stdin/stdout via VfsActorRuntime** (`actor_runtime_mocked` crate).  
   The VFS assigns fds by call order, so open handles in the right sequence:
   ```rust
   let runtime = VfsActorRuntime::new();
   runtime.add_file("stdin".to_string(), spec_bytes);
   let _ = runtime.open_read("stdin");   // → handle 0 = stdin
   let _ = runtime.open_write("stdout"); // → handle 1 = stdout
   ```
   After the call, read output with `runtime.get_file("stdout")`.

2. **Mock HTTP transport** via ureq's `unversioned::transport` traits.  
   Implement `FakeConnector` (returns a `FakeTransport`) and build the agent:
   ```rust
   let agent = ureq::Agent::with_parts(
       ureq::config::Config::default(),
       FakeConnector { response: b"HTTP/1.1 200 OK\r\n...\r\n\r\nbody".to_vec() },
       ureq::unversioned::resolver::DefaultResolver::default(),
   );
   ```
   `FakeTransport` feeds the raw HTTP response bytes through `await_input`; `transmit_output` can be a no-op.  
   See `tests/execute_test.rs` for the full implementation.

## Key dependencies

| Crate | Role |
|-------|------|
| `ureq` v3 | HTTP client; use `unversioned::transport` traits for mocking |
| `actor_runtime` | `ActorRuntime` trait + `StdHandle` enum |
| `actor_runtime_mocked` | `VfsActorRuntime` for tests (dev-dep only) |
| `actor_io` | `AReader` / `AWriter` |
| `serde_json` | Parses the JSON spec from stdin |
