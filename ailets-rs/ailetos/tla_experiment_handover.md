# TLA+ Formal Verification Experiment - Handover

## Experiment Goal

Validate whether TLA+ formal verification can detect the race conditions in `pool.rs` that were manually discovered through code analysis.

This branch (`tla-experiment`) contains the **buggy code** from commit `5ac954c`, before the fixes in commit `943e9e3`. We will write a TLA+ specification of the buggy algorithm and use the TLC model checker to see if it finds the known race conditions.

## Context

### What We Know

From `pipepool_race_conditions_handover.md`, we identified 6 race conditions in `get_or_await_reader()`:

| Race | Severity | Description |
|------|----------|-------------|
| #1 | Low | Missed notification (already mitigated by loop-and-recheck) |
| #2 | **Critical** | Latent created after writer exists → deadlock |
| #3 | Medium | DAG state changes between check and use |
| #4 | Medium | Multiple readers create duplicate latents → resource leak |
| #5 | Low | Orphaned latent from terminated node |
| #6 | **Critical** | Root cause: no recheck before pushing latent |

**Primary targets:** Race #2 and #4 (critical deadlock and resource leak)

### The Buggy Code

At this commit (`5ac954c`), the code in `src/pipe/pool.rs` lines 291-299 looks like:

```rust
{
    let mut inner = self.inner.lock();
    inner.latent_writers.push(latent);  // ← NO RECHECK!
}
debug!(key = ?key, "created latent writer");

// Wait for the pipe to be realized
notify.notified().await;
continue;
```

**Bug:** Between checking pool state (line 221) and pushing latent (line 293), another thread could have created a writer or latent. No recheck happens.

## Experiment Setup

### Step 1: Install TLA+ Toolbox

```bash
# Download from: https://github.com/tlaplus/tlaplus/releases
# Or use command line tools:
wget https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/TLAToolbox-1.8.0-linux.gtk.x86_64.zip
unzip TLAToolbox-1.8.0-linux.gtk.x86_64.zip
```

Alternative: Use command-line TLC:
```bash
wget https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar
alias tlc='java -XX:+UseParallelGC -cp tla2tools.jar tlc2.TLC'
```

### Step 2: Create TLA+ Specification

Create file `spec/PipePool.tla` with the specification below.

### Step 3: Run Model Checker

```bash
# In TLA+ Toolbox: Model → Run TLC
# Or command line:
tlc spec/PipePool.tla -workers auto
```

### Step 4: Analyze Results

Compare TLC output with known race conditions. Does TLC find Race #2 and #4?

## TLA+ Specification Skeleton

Create `spec/PipePool.tla`:

```tla
--------------------------- MODULE PipePool ---------------------------
(*
  Specification of pipe pool latent writer creation logic.

  This models the buggy code from commit 5ac954c to validate
  whether TLC can detect Race #2 and #4.
*)

EXTENDS Integers, Sequences, FiniteSets, TLC

-----------------------------------------------------------------------------
(* CONSTANTS *)

CONSTANTS
  Readers,      \* Set of reader thread IDs (e.g., {r1, r2})
  Writers,      \* Set of writer thread IDs (e.g., {w1})
  Keys          \* Set of pipe keys (e.g., {k1})

(* Assumes at least 2 readers for Race #4 *)
ASSUME Cardinality(Readers) >= 2

-----------------------------------------------------------------------------
(* VARIABLES *)

VARIABLES
  \* Pool state
  pool_writers,      \* [key -> Writer] - realized writers
  pool_latents,      \* [key -> Set of Notify handles] - latent writers

  \* Reader state
  reader_state,      \* [reader -> state] where state in {idle, checking, waiting, done}
  reader_key,        \* [reader -> key] - which key reader is requesting
  reader_notify,     \* [reader -> notify handle] - notify handle reader is waiting on
  reader_result,     \* [reader -> {none, some, waiting}] - reader result

  \* Thread scheduling
  pc,                \* [thread -> program counter] - current execution point

  \* Notify mechanism
  notified           \* [notify_handle -> BOOLEAN] - has this handle been notified?

vars == <<pool_writers, pool_latents, reader_state, reader_key,
          reader_notify, reader_result, pc, notified>>

-----------------------------------------------------------------------------
(* TYPE INVARIANT *)

TypeOK ==
  /\ pool_writers \in [Keys -> BOOLEAN]  \* TRUE if writer exists
  /\ pool_latents \in [Keys -> SUBSET Nat]  \* Set of notify handles (using Nat as handles)
  /\ reader_state \in [Readers -> {"idle", "checked_pool", "decided_create", "done"}]
  /\ reader_result \in [Readers -> {"none", "some", "waiting"}]

-----------------------------------------------------------------------------
(* INITIAL STATE *)

Init ==
  /\ pool_writers = [k \in Keys |-> FALSE]
  /\ pool_latents = [k \in Keys |-> {}]
  /\ reader_state = [r \in Readers |-> "idle"]
  /\ reader_key = [r \in Readers |-> CHOOSE k \in Keys : TRUE]  \* arbitrary initial value
  /\ reader_notify = [r \in Readers |-> 0]
  /\ reader_result = [r \in Readers |-> "waiting"]
  /\ pc = [t \in (Readers \cup Writers) |-> "start"]
  /\ notified = [h \in Nat |-> FALSE]

-----------------------------------------------------------------------------
(* READER ACTIONS *)

(* Reader checks pool state under lock and decides what to do *)
ReaderCheckPool(r, k) ==
  /\ reader_state[r] = "idle"
  /\ pc[r] = "start"
  /\ reader_key' = [reader_key EXCEPT ![r] = k]
  /\ IF pool_writers[k]
     THEN
       \* Writer exists - return immediately
       /\ reader_result' = [reader_result EXCEPT ![r] = "some"]
       /\ reader_state' = [reader_state EXCEPT ![r] = "done"]
       /\ pc' = [pc EXCEPT ![r] = "done"]
       /\ UNCHANGED <<pool_writers, pool_latents, reader_notify, notified>>
     ELSE IF pool_latents[k] # {}
     THEN
       \* Latent exists - wait on it
       /\ LET handle == CHOOSE h \in pool_latents[k] : TRUE
          IN
            /\ reader_notify' = [reader_notify EXCEPT ![r] = handle]
            /\ reader_state' = [reader_state EXCEPT ![r] = "checked_pool"]
            /\ pc' = [pc EXCEPT ![r] = "await_notify"]
       /\ UNCHANGED <<pool_writers, pool_latents, reader_result, notified>>
     ELSE
       \* Nothing exists - will create latent
       /\ reader_state' = [reader_state EXCEPT ![r] = "checked_pool"]
       /\ pc' = [pc EXCEPT ![r] = "create_latent"]
       /\ UNCHANGED <<pool_writers, pool_latents, reader_notify, reader_result, notified>>

(* Reader creates latent - THIS IS THE BUGGY STEP (no recheck!) *)
ReaderCreateLatent(r) ==
  /\ reader_state[r] = "checked_pool"
  /\ pc[r] = "create_latent"
  /\ LET k == reader_key[r]
         handle == r  \* Use reader ID as notify handle for simplicity
     IN
       \* BUG: No recheck of pool_writers or pool_latents here!
       /\ pool_latents' = [pool_latents EXCEPT ![k] = @ \cup {handle}]
       /\ reader_notify' = [reader_notify EXCEPT ![r] = handle]
       /\ reader_state' = [reader_state EXCEPT ![r] = "waiting"]
       /\ pc' = [pc EXCEPT ![r] = "await_notify"]
  /\ UNCHANGED <<pool_writers, reader_result, notified>>

(* Reader waits on notification *)
ReaderAwaitNotify(r) ==
  /\ pc[r] = "await_notify"
  /\ LET handle == reader_notify[r]
     IN
       /\ notified[handle] = TRUE
       /\ reader_state' = [reader_state EXCEPT ![r] = "idle"]
       /\ pc' = [pc EXCEPT ![r] = "start"]  \* Loop back
  /\ UNCHANGED <<pool_writers, pool_latents, reader_key, reader_notify, reader_result, notified>>

-----------------------------------------------------------------------------
(* WRITER ACTIONS *)

(* Writer creates writer and notifies latent waiters *)
WriterCreateWriter(w, k) ==
  /\ pc[w] = "start"
  /\ ~pool_writers[k]  \* Writer doesn't exist yet
  /\ pool_writers' = [pool_writers EXCEPT ![k] = TRUE]
  /\ LET handles == pool_latents[k]
     IN
       \* Notify all waiters
       /\ notified' = [h \in Nat |-> IF h \in handles THEN TRUE ELSE notified[h]]
       \* Remove latent
       /\ pool_latents' = [pool_latents EXCEPT ![k] = {}]
  /\ pc' = [pc EXCEPT ![w] = "done"]
  /\ UNCHANGED <<reader_state, reader_key, reader_notify, reader_result>>

-----------------------------------------------------------------------------
(* NEXT STATE *)

Next ==
  \/ \E r \in Readers, k \in Keys : ReaderCheckPool(r, k)
  \/ \E r \in Readers : ReaderCreateLatent(r)
  \/ \E r \in Readers : ReaderAwaitNotify(r)
  \/ \E w \in Writers, k \in Keys : WriterCreateWriter(w, k)

Spec == Init /\ [][Next]_vars

-----------------------------------------------------------------------------
(* INVARIANTS TO CHECK *)

(* CRITICAL: Writer and latent should never coexist for same key *)
NoCoexistence ==
  \A k \in Keys :
    ~(pool_writers[k] /\ pool_latents[k] # {})

(* CRITICAL: At most one latent per key (prevents duplicate latents) *)
NoDuplicateLatents ==
  \A k \in Keys :
    Cardinality(pool_latents[k]) <= 1

(* Sanity check: readers eventually complete *)
ReadersEventuallyComplete ==
  \A r \in Readers :
    <>(reader_result[r] \in {"none", "some"})

-----------------------------------------------------------------------------
(* MODEL CONFIGURATION *)

(* For TLC model checker - specify small constants *)
ModelReaders == {r1, r2}           \* 2 readers to test Race #4
ModelWriters == {w1}               \* 1 writer
ModelKeys == {k1}                  \* 1 key is sufficient

=============================================================================
```

## Model Configuration

Create `spec/PipePool.cfg`:

```cfg
CONSTANTS
  Readers = {r1, r2}
  Writers = {w1}
  Keys = {k1}

INVARIANTS
  TypeOK
  NoCoexistence
  NoDuplicateLatents

PROPERTIES
  ReadersEventuallyComplete

CONSTRAINT
  \* Limit state space exploration
  Cardinality(DOMAIN notified) < 10
```

## Expected Results

### Race #2: Writer and Latent Coexist

**Expected:** TLC should find a trace where `NoCoexistence` is violated.

**Scenario:**
1. Reader r1: Check pool (no writer, no latent) → decide to create latent
2. Writer w1: Create writer for k1
3. Reader r1: Push latent for k1 (no recheck!)
4. **Violation:** Both `pool_writers[k1] = TRUE` and `pool_latents[k1] # {}`

**TLC output:**
```
Error: Invariant NoCoexistence is violated.
State trace:
1. Initial state
2. r1: ReaderCheckPool(r1, k1) -> decided to create latent
3. w1: WriterCreateWriter(w1, k1) -> writer created
4. r1: ReaderCreateLatent(r1) -> latent created
   pool_writers[k1] = TRUE
   pool_latents[k1] = {r1}
   ^^^ INVARIANT VIOLATED
```

### Race #4: Duplicate Latents

**Expected:** TLC should find a trace where `NoDuplicateLatents` is violated.

**Scenario:**
1. Reader r1: Check pool (nothing exists) → decide to create latent
2. Reader r2: Check pool (nothing exists) → decide to create latent
3. Reader r1: Push latent with handle r1
4. Reader r2: Push latent with handle r2
5. **Violation:** `pool_latents[k1] = {r1, r2}` (cardinality > 1)

**TLC output:**
```
Error: Invariant NoDuplicateLatents is violated.
State trace:
1. Initial state
2. r1: ReaderCheckPool(r1, k1) -> decided to create latent
3. r2: ReaderCheckPool(r2, k1) -> decided to create latent
4. r1: ReaderCreateLatent(r1) -> latent r1 created
5. r2: ReaderCreateLatent(r2) -> latent r2 created
   pool_latents[k1] = {r1, r2}
   Cardinality = 2
   ^^^ INVARIANT VIOLATED
```

## Success Criteria

✅ **Experiment succeeds if:**
1. TLC finds violation of `NoCoexistence` (Race #2)
2. TLC finds violation of `NoDuplicateLatents` (Race #4)
3. Counterexample traces match expected scenarios above

❌ **Experiment fails if:**
1. TLC reports "No errors found" (missed the bugs)
2. TLC finds different bugs we didn't anticipate
3. Spec doesn't model the algorithm correctly

## Troubleshooting

### TLC reports "No errors found"

**Problem:** Spec doesn't model the race window correctly

**Fix:** Ensure `ReaderCreateLatent` can interleave with `WriterCreateWriter` between `ReaderCheckPool` and `ReaderCreateLatent`

### TLC runs forever / state space explosion

**Problem:** Too many states to explore

**Fix:**
- Reduce constants (1 key, 2 readers is sufficient)
- Add state constraints
- Use symmetry reduction

### Spec syntax errors

**Solution:** Use TLA+ Toolbox which has syntax highlighting and error checking

## Next Steps After Experiment

### If TLC finds the bugs ✅

1. **Document results:** Screenshot of TLC error traces
2. **Compare to fixed code:** Check out commit `943e9e3` with the recheck fix
3. **Update spec:** Add recheck logic to `ReaderCreateLatent`
4. **Re-run TLC:** Verify invariants now hold
5. **Decision:** Consider adding TLA+ to CI/CD pipeline

### If TLC doesn't find the bugs ❌

1. **Debug spec:** Review the state machine carefully
2. **Simplify:** Start with even smaller model (1 reader, no DAG state)
3. **Iterate:** Gradually add complexity until race appears
4. **Re-evaluate:** Maybe TLA+ isn't suitable for this particular code

### If experiment is successful

Consider expanding to:
- Model Race #3 (DAG state staleness)
- Add liveness properties (readers don't hang forever)
- Model the full pool lifecycle (close, shutdown)
- Write specs for other concurrent components

## References

- **Buggy code:** This branch at commit `5ac954c`
- **Fixed code:** Branch `a217-value-node-is-kv-entry` at commit `943e9e3`
- **Race analysis:** `pipepool_race_conditions_handover.md`
- **TLA+ resources:**
  - [Learn TLA+](https://learntlaplus.com/)
  - [TLA+ Hyperbook](https://www.learntla.com/)
  - [TLA+ Examples](https://github.com/tlaplus/Examples)

## Files to Create

```
spec/
├── PipePool.tla       # Main specification (provided above)
├── PipePool.cfg       # Model configuration (provided above)
└── README.md          # This handover document (copy this file)
```

## Time Estimate

- **Writing spec:** 2-4 hours (first time)
- **Running TLC:** Minutes (small state space)
- **Debugging spec:** 2-4 hours (if needed)
- **Total:** 1 day

## Experiment Log

Record your findings here:

### Date: _________

**Investigator:** _________

**TLC Version:** _________

**Results:**
- [ ] NoCoexistence violated? Y/N
  - Trace: _________
- [ ] NoDuplicateLatents violated? Y/N
  - Trace: _________

**Conclusion:**

**Recommendation:**

---

**Document prepared by:** Claude Code
**Date:** 2026-03-26
**Branch:** `tla-experiment`
**Base commit:** `5ac954c` (buggy code before fixes)
