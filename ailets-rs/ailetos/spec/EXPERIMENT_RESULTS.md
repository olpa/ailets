# TLA+ Experiment Results

## Summary

**✅ EXPERIMENT SUCCESSFUL**

TLC model checker successfully detected both critical race conditions in the buggy pool.rs code.

---

## Configuration

- **Date:** 2026-03-27
- **TLC Version:** 2026.03.27.000708
- **Branch:** `tla-experiment`
- **Base Commit:** `5ac954c` (buggy code before fixes)
- **Model:** 2 readers (`r1`, `r2`), 1 writer (`w1`), 1 key (`k1`)

---

## Results

### Race #2: Writer and Latent Coexist ✅ DETECTED

**Invariant violated:** `NoCoexistence`

**What happened:**
```
State 1: Initial state (no writer, no latent)
State 2: Reader r2 checks pool → no writer exists → decides to create latent
State 3: Writer w1 creates writer (pool_writers[k1] = TRUE)
State 4: Reader r2 pushes latent WITHOUT RECHECK (pool_latents[k1] = {r2})

VIOLATION: Both pool_writers[k1] = TRUE AND pool_latents[k1] = {r2}
```

**Impact:** Deadlock - reader r2 waits forever on a latent that will never be notified because the writer already exists.

**Root cause:** No recheck between checking pool state (State 2) and pushing latent (State 4).

**TLC output:**
- 24 states generated, 23 distinct
- Depth: 4
- Time: < 1 second

---

### Race #4: Duplicate Latents ✅ DETECTED

**Invariant violated:** `NoDuplicateLatents`

**What happened:**
```
State 1: Initial state (no writer, no latent)
State 2: Reader r1 checks pool → no latent exists → decides to create latent
State 3: Reader r2 checks pool → no latent exists → decides to create latent
State 4: Reader r1 pushes latent (pool_latents[k1] = {r1})
State 5: Reader r2 pushes latent WITHOUT RECHECK (pool_latents[k1] = {r1, r2})

VIOLATION: Cardinality(pool_latents[k1]) = 2
```

**Impact:** Resource leak - multiple notify handles created for the same key, only one will ever be used.

**Root cause:** No recheck between checking pool state (States 2-3) and pushing latent (States 4-5).

**TLC output:**
- 43 states generated, 38 distinct
- Depth: 6
- Time: < 1 second

---

## Conclusion

### The Experiment Validates That:

1. **TLA+ formal verification CAN detect concurrency bugs** in complex Rust code
2. **Both critical races were found** exactly as predicted in the handover document
3. **Counterexample traces match expected scenarios** from manual analysis
4. **The bug is precisely identified:** Missing recheck in `ReaderCreateLatent` action

### The Root Cause (lines 112-124 of PipePool.tla):

```tla
ReaderCreateLatent(r) ==
  /\ reader_state[r] = "decided_create"
  /\ pc[r] = "create_latent"
  /\ LET k == reader_key[r]
         handle == r
     IN
       \* BUG: No recheck of pool_writers or pool_latents here!
       /\ pool_latents' = [pool_latents EXCEPT ![k] = @ \cup {handle}]
       ...
```

This corresponds to the buggy Rust code (pool.rs:291-299):
```rust
{
    let mut inner = self.inner.lock();
    inner.latent_writers.push(latent);  // ← NO RECHECK!
}
```

---

## Next Steps

### Verification of Fix

1. Check out commit `943e9e3` with the recheck fix
2. Update TLA+ spec to add recheck logic before pushing latent:
   ```tla
   /\ IF pool_writers[k] \/ pool_latents[k] # {}
      THEN (* abort, loop back *)
      ELSE (* create latent *)
   ```
3. Re-run TLC - should find no violations

### Broader Application

Consider using TLA+ for:
- Modeling Race #3 (DAG state staleness)
- Adding liveness properties (readers don't hang forever)
- Modeling full pool lifecycle (close, shutdown)
- Specifying other concurrent components

---

## Performance Notes

- State space exploration was very fast (< 1 second per run)
- Small model (2 readers, 1 writer, 1 key) was sufficient to find bugs
- Breadth-first search found violations within 4-6 state transitions
- No state explosion issues with this model size

---

## Files Generated

- `PipePool_TTrace_1774616191.tla` - Trace for Race #2
- `PipePool_TTrace_1774616254.tla` - Trace for Race #4

---

**Experiment conducted by:** Claude Code
**Status:** ✅ Complete and successful
