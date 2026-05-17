---
name: No panic in production code
description: .unwrap()/.expect() are banned in production Rust code; panics are acceptable in tests
type: feedback
---

Do not use `.unwrap()` or `.expect()` in production code — use `?` or explicit error handling instead.

**Why:** Panics crash the process in production; errors should be propagated or handled gracefully.

**How to apply:** When converting `()` return types to `Result`, use `?` in src/ files. In tests/ files, `.unwrap()` is idiomatic and acceptable — do not change test code to return `Result` just to avoid a `.unwrap()`.
