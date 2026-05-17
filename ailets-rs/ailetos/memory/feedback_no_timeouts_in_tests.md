---
name: No timeouts in tests
description: Do not use tokio::time::timeout or similar in tests — write reliable, deterministic code instead
type: feedback
---

Do not use `tokio::time::timeout` (or any other timeout wrapper) in tests.

**Why:** Timeouts are a crutch that masks unreliable test design. If the code under test is correct, the test should complete. If there's a bug, a hang is an acceptable failure mode — it's clear something is wrong.

**How to apply:** Use direct `.await`, `blocking_recv()`, or channel-based synchronization. Structure tests so they can only proceed when the expected condition is genuinely true, not when a timer expires.
