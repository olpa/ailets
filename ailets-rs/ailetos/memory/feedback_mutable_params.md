---
name: No mutable parameters
description: Avoid &mut T parameters; prefer returning values instead
type: feedback
---

Do not write functions with `&mut T` parameters. Instead, have the function return the modified value (or a new value), and let the caller reassign.

**Why:** User prefers the functional style — return values over mutation through references.

**How to apply:** Replace `fn foo(x: &mut T)` with `fn foo(x: T) -> T` or `fn foo(...) -> T`. For `&mut self` on receivers that require it (e.g. channel recv), that is acceptable since it's inherent to the type.
