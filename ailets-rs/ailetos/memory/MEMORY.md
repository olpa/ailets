# Memory Index

- [Always ask before committing](feedback_commit_permission.md) — Never run git commit without explicit user permission
- [No panic in production code](feedback_no_panic_production.md) — .unwrap()/.expect() banned in src/; fine in tests/
- [expect() message style](feedback_expect_messages.md) — messages should describe the failure; prefer .unwrap() in tests
- [No mutable parameters](feedback_mutable_params.md) — avoid &mut T params; return values instead
- [No timeouts in tests](feedback_no_timeouts_in_tests.md) — write reliable/deterministic tests; never use tokio::time::timeout or similar
