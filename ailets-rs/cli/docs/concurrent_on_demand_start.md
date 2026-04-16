# Concurrent On-Demand Start

Demo script: `scripts/concurrent_on_demand_start.dagsh`

## Purpose

Show two fundamental runtime properties of the actor scheduler using only
`shell_input` and `dbg` nodes — no external tooling required.

## Properties demonstrated

### 1. On-demand start

An actor does not start until data actually arrives at its input.  With a
`shell_input` source, downstream actors remain `NotStarted` until the shell
feeds bytes.  In the chain `shell_input → dbg_1 → dbg_2`:

* `dbg_1` stays idle until `shell_input` writes its first byte.
* `dbg_2` stays idle until `dbg_1` has forwarded at least one byte.

### 2. Concurrent execution

Multiple actors can be alive (and suspended mid-stream) at the same time.
`dbg` actors pause after forwarding N bytes via `--bytes-before-pause=N`.
Because `dbg_1` flushes its first chunk before suspending, `dbg_2` starts
while `dbg_1` is still suspended — both actors are in-flight simultaneously.

## DAG topology

```
shell_input  →  dbg_1 (pause after 5 B)  →  dbg_2 (pause after 3 B)
```

Input: `"hello world"` (11 bytes)

| Phase | shell_input | dbg_1 | dbg_2 |
|-------|-------------|-------|-------|
| After `run --bg`, before `write` | NotStarted¹ | NotStarted | NotStarted |
| After `write` + `close`, mid-run | Terminated | Suspended (forwarded "hello") | Suspended (forwarded "hel") |
| After `resume` + `fg` | — | Terminated | Terminated |

¹ The scheduler thread starts shell_input almost immediately, but the shell's
`status` command races the background thread and often sees it as NotStarted.

## Script flow

1. Build the three-node DAG and `show` it.
2. `run --bg` — the scheduler thread is live but no data has arrived.
3. `status` — likely shows all three nodes NotStarted (confirms no eager start).
4. `write $src "hello world"` + `close $src` — shell_input delivers 11 bytes
   then closes.  The scheduler wakes `dbg_1`, which forwards "hello" (5 B)
   to `dbg_2`, then calls `suspend_and_wait`.  Meanwhile `dbg_2` has started
   (Property 1 visible in log) and forwards "hel" (3 B), then also suspends
   (Property 2: both suspended simultaneously).
5. `status` — races the background thread; may still show NotStarted.
6. `resume $dbg1` + `resume $dbg2` — pre-signal both actors (idempotent: if
   called before the actor suspends, the next `suspend_and_wait` skips blocking).
7. `fg` — wait for completion.  **Read the log output here** — it shows both
   actors starting, hitting their pause points, and resuming concurrently.
8. Final `status` — all nodes terminated.

## Where to observe the properties

The `status` commands race the background scheduler thread, so they may not
capture the exact mid-run state.  The authoritative evidence is in the `fg`
log output:

**Property 1 — on-demand start:**
```
INFO dbg actor starting  node=2   ← dbg_1 starts after shell_input writes
...
INFO dbg actor starting  node=3   ← dbg_2 starts only after dbg_1 forwards bytes
```

**Property 2 — concurrent execution:**
```
INFO dbg actor reached pause threshold  node=2  bytes_copied=5
INFO dbg actor starting                 node=3   ← dbg_2 starts before dbg_1 resumes
INFO dbg actor reached pause threshold  node=3  bytes_copied=3
INFO dbg actor resumed, continuing      node=2   ← both were paused simultaneously
INFO dbg actor resumed, continuing      node=3
```

## Expected final status

```
Nodes: 3 total, 0 not started, 0 running, 0 suspended, 3 terminated
```

## Infrastructure note

This script relies on `resume` being idempotent for self-suspending actors:
calling `resume` before an actor reaches `suspend_and_wait` pre-signals the
handle, so the actor skips waiting when it gets there.  This is implemented
via the `pre_resumed` set in `ailetos::suspension::SuspensionState`.
