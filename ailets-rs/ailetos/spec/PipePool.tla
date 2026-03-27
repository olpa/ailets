--------------------------- MODULE PipePool ---------------------------
(*
  Specification of pipe pool latent writer creation logic.

  This models the buggy code from commit 5ac954c to validate
  whether TLC can detect Race #2 and #4.

  Race #2: Latent created after writer exists (deadlock)
  Race #4: Multiple readers create duplicate latents (resource leak)
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
  pool_writers,      \* [key -> BOOLEAN] - TRUE if realized writer exists
  pool_latents,      \* [key -> Set of notify handles] - latent writers

  \* Reader state
  reader_state,      \* [reader -> state] where state in {idle, checked_pool, waiting, done}
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
  /\ pool_writers \in [Keys -> BOOLEAN]
  /\ pool_latents \in [Keys -> SUBSET Readers]  \* Using Readers as notify handles
  /\ reader_state \in [Readers -> {"idle", "checked_pool", "decided_create", "waiting", "done"}]
  /\ reader_result \in [Readers -> {"none", "some", "waiting"}]
  /\ pc \in [Readers \cup Writers -> STRING]

-----------------------------------------------------------------------------
(* INITIAL STATE *)

Init ==
  /\ pool_writers = [k \in Keys |-> FALSE]
  /\ pool_latents = [k \in Keys |-> {}]
  /\ reader_state = [r \in Readers |-> "idle"]
  /\ reader_key = [r \in Readers |-> CHOOSE k \in Keys : TRUE]
  /\ reader_notify = [r \in Readers |-> r]  \* Use reader as its own notify handle
  /\ reader_result = [r \in Readers |-> "waiting"]
  /\ pc = [t \in (Readers \cup Writers) |-> "start"]
  /\ notified = [r \in Readers |-> FALSE]

-----------------------------------------------------------------------------
(* READER ACTIONS *)

(*
  Reader checks pool state under lock.
  Corresponds to pool.rs lines 221-250 (inside lock)
*)
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
       \* Latent exists - wait on it (simplified: just grab any handle)
       /\ LET handle == CHOOSE h \in pool_latents[k] : TRUE
          IN
            /\ reader_notify' = [reader_notify EXCEPT ![r] = handle]
            /\ reader_state' = [reader_state EXCEPT ![r] = "waiting"]
            /\ pc' = [pc EXCEPT ![r] = "await_notify"]
       /\ UNCHANGED <<pool_writers, pool_latents, reader_result, notified>>
     ELSE
       \* Nothing exists - decide to create latent
       /\ reader_state' = [reader_state EXCEPT ![r] = "decided_create"]
       /\ pc' = [pc EXCEPT ![r] = "create_latent"]
       /\ UNCHANGED <<pool_writers, pool_latents, reader_notify, reader_result, notified>>

(*
  Reader creates latent - THE BUGGY STEP!
  Corresponds to pool.rs lines 281-295

  BUG: No recheck between ReaderCheckPool and here!
  Another thread could have created writer or latent in between.
*)
ReaderCreateLatent(r) ==
  /\ reader_state[r] = "decided_create"
  /\ pc[r] = "create_latent"
  /\ LET k == reader_key[r]
         handle == r  \* Use reader ID as notify handle
     IN
       \* BUG: No recheck of pool_writers or pool_latents here!
       \* This allows writer to be created between check and push
       \* This allows duplicate latents from concurrent readers
       /\ pool_latents' = [pool_latents EXCEPT ![k] = @ \cup {handle}]
       /\ reader_notify' = [reader_notify EXCEPT ![r] = handle]
       /\ reader_state' = [reader_state EXCEPT ![r] = "waiting"]
       /\ pc' = [pc EXCEPT ![r] = "await_notify"]
  /\ UNCHANGED <<pool_writers, reader_key, reader_result, notified>>

(*
  Reader waits on notification.
  Corresponds to pool.rs line 298: notify.notified().await
*)
ReaderAwaitNotify(r) ==
  /\ pc[r] = "await_notify"
  /\ reader_state[r] = "waiting"
  /\ LET handle == reader_notify[r]
     IN
       /\ notified[handle] = TRUE
       /\ reader_state' = [reader_state EXCEPT ![r] = "idle"]
       /\ pc' = [pc EXCEPT ![r] = "start"]  \* Loop back to check again
  /\ UNCHANGED <<pool_writers, pool_latents, reader_key, reader_notify, reader_result, notified>>

-----------------------------------------------------------------------------
(* WRITER ACTIONS *)

(*
  Writer creates writer and notifies latent waiters.
  Corresponds to pool.rs touch_writer lines 361-411
*)
WriterCreateWriter(w, k) ==
  /\ pc[w] = "start"
  /\ ~pool_writers[k]  \* Writer doesn't exist yet
  /\ pool_writers' = [pool_writers EXCEPT ![k] = TRUE]
  /\ LET handles == pool_latents[k]
     IN
       \* Notify all waiters (remove latent and notify)
       /\ notified' = [h \in Readers |-> IF h \in handles THEN TRUE ELSE notified[h]]
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
(* INVARIANTS - These should be violated by the buggy code! *)

(*
  CRITICAL INVARIANT: Writer and latent should never coexist for same key.

  Race #2 violates this when:
  1. Reader checks pool (no writer)
  2. Writer creates writer
  3. Reader pushes latent (no recheck!)
  Result: Both pool_writers[k] = TRUE and pool_latents[k] # {}
*)
NoCoexistence ==
  \A k \in Keys :
    ~(pool_writers[k] /\ pool_latents[k] # {})

(*
  CRITICAL INVARIANT: At most one latent per key.

  Race #4 violates this when:
  1. Reader r1 checks pool (no latent)
  2. Reader r2 checks pool (no latent)
  3. Reader r1 pushes latent
  4. Reader r2 pushes latent
  Result: pool_latents[k] = {r1, r2}
*)
NoDuplicateLatents ==
  \A k \in Keys :
    Cardinality(pool_latents[k]) <= 1

(*
  Sanity check: If writer exists or latent is closed,
  readers shouldn't be stuck waiting forever.
  (This is a simplified liveness property)
*)
NoStuckReaders ==
  \A r \in Readers :
    (reader_state[r] = "waiting") =>
      \E k \in Keys :
        \/ pool_writers[k]
        \/ k \in DOMAIN pool_latents /\ r \in pool_latents[k]

=============================================================================
