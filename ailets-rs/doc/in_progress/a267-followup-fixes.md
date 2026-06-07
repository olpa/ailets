# A267 follow-up: fix issues found in code review of `a267-hang-on-never-unblocked`

## Context

A code review of the branch's diff (mainly `ailetos/src/executor.rs` and
`ailetos/src/dag.rs`) found one real correctness bug — ironically a *new* hang
in the very code that was meant to fix hangs — plus two minor cleanups.

## 1. (Must fix) `spawn_ready_actors` can starve a Ready node forever

**File:** `ailetos/src/executor.rs`, `spawn_ready_actors` (~line 318) and
`run_spawn_loop_jobs` (~line 457).

`spawn_ready_actors` iterates `pending: &HashSet<Handle>` in hash order
(effectively nondeterministic across runs — `RandomState`). As soon as **any**
node yields `SpawnReadiness::FailedDependency(dep)`, it immediately returns
`SpawnOutcome::FailedNodeDependency(dep)` — abandoning the scan. Any other node
in the same batch that is `Ready` (already collected into `to_spawn`, or not
yet visited) is neither spawned nor re-queued; it's simply dropped for this
iteration.

Back in `run_spawn_loop_jobs`, the `FailedNodeDependency` branch calls
`remove_blocked_from_pending` (which only removes nodes that transitively
depend on `failed_dep`) and then **does not call `infra.executor_wakeup.send(())`**.
The loop falls straight into `tokio::select!` and blocks on `job_rx.recv()`,
`wakeup_rx.changed()`, or `actor_tasks.join_next()`.

`executor_wakeup.send(())` is sent from exactly two places (confirmed by grep):
after a node terminates, and after marking an unregistered actor as terminated.
Neither is guaranteed to fire after this branch.

### Concrete failure scenario

1. Node `A` is already `Terminated` with a non-zero exit code.
2. Node `B` depends on `A` → `is_ready_to_spawn(B)` returns `FailedDependency(A)`.
3. Node `C` has no dependency on `A` and is `Ready`.
4. `B` and `C` land in `pending` from the same submit batch.
5. If `HashSet` iteration visits `B` before `C`, `spawn_ready_actors` returns
   early — `C` is never spawned. `remove_blocked_from_pending` strips `B` but
   leaves `C` untouched in `pending`. No wakeup is sent.
6. `C` sits `Ready` in `pending` forever, until some unrelated event (another
   job submission, another node's termination) happens to trigger
   `executor_wakeup` — a real, nondeterministic hang.

### Fix direction

Either:
- Make `spawn_ready_actors` finish classifying the whole batch — spawn all
  `Ready` nodes regardless of whether a `FailedDependency` was also seen, and
  report the failed dependency (if any) afterward, or
- Send `executor_wakeup` after `remove_blocked_from_pending` so the next loop
  iteration re-evaluates the remaining `pending` set (including `C`) — note
  this only fully closes the gap if it's combined with not discarding
  already-classified `Ready` nodes, since a *third* always-failing node could
  again precede `C` in iteration order on the retry.

Add a regression test that reproduces the scenario above (mixed
ready/failed-dependency nodes in one `pending` batch) and asserts the ready
node is spawned promptly (not just "eventually, by accident").

## 2. (Minor) Cosmetic: stray leading space when a `NotStarted`+suspended node is not in `pending`

**File:** `ailetos/src/dag.rs`, `format_state_symbol` (~line 136) /
`state_bracket` construction (~line 136-140).

`format_state_symbol` now returns `""` for `NodeState::NotStarted` when the
node isn't in the `pending` set passed to `dump`/`dump_colored`. If such a node
is also suspended (`is_suspended` doesn't check node state), `suspended_suffix`
is `" ⏸ suspended"` (leading space) and `state_symbol` is `""`, producing:

```
[ ⏸ suspended]
```

with a stray leading space inside the brackets. Previously
`NodeState::NotStarted` always rendered a non-empty `"⋯ pending"` symbol, so
this combination couldn't occur — it's a small regression introduced by this
diff. Trim the suffix or special-case the empty-symbol + suspended combination.

## 3. (Minor) Stale doc comment on `spawn_ready_actors`

**File:** `ailetos/src/executor.rs`, ~line 303.

```rust
/// Spawn one batch of ready nodes from `pending`.
///
/// Spawns actor tasks directly into `actor_tasks` `JoinSet`.
/// Returns the set of nodes that were not ready to spawn (still pending).
fn spawn_ready_actors(...) -> SpawnOutcome {
```

The doc comment still describes the old `HashSet<Handle>` return value; the
function now returns `SpawnOutcome` (`Ok(remaining)` or
`FailedNodeDependency(Handle)`). Update the comment to describe both variants —
especially the early-return behavior, since that's exactly where bug #1 lives.

## Acceptance criterion

A new regression test demonstrating the mixed ready/blocked batch from §1
passes, and `cargo test -p ailetos` is green.
