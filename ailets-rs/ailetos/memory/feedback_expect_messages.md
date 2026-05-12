---
name: expect() message style
description: .expect() messages should describe the failure, not sound like an assertion about the expected outcome
type: feedback
---

Avoid `.expect("submit failed")` — it reads as "this is expected to fail." Use `.unwrap()` in test code where no explanation is needed, or write the message as a failure description: `.expect("channel unexpectedly closed")`.

**Why:** The user found `.expect("submit failed")` confusing — it sounds like an expectation/assertion rather than an error label.

**How to apply:** In tests, prefer `.unwrap()` for simple operations. If a message adds value, write it from the failure perspective: "X unexpectedly Y" or "could not X".
