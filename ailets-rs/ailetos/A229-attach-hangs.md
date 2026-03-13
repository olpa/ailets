# A229 Attach Hangs - Debug Handover

## Problem

Running `RUST_LOG=trace cargo run -p cli` hangs (reported issue).

## Added Trace Logging

Trace-level logging was added for debugging:

1. **Service lifecycle**: Creation/initialization (mentioning stored dependencies) and destruction
2. **Notification queue rx loops**: Before loop, message received, after loop

### Files Modified

- `system_runtime.rs` - SystemRuntime init/destroy, request_rx loop
- `pipepool.rs` - PipePool init, get_or_await_reader loop with notify
- `stub_actor_runtime.rs` - BlockingActorRuntime init/shutdown, FdTable init
- `attachments.rs` - AttachmentManager init, attach_to_stdout/stderr loops
- `io/flush_coordinator.rs` - FlushCoordinator init/drop, writer_loop
- `environment.rs` - Environment init, ActorRegistry init
- `dag.rs` - Dag init
- `idgen.rs` - IdGen init
- `notification_queue.rs` - NotificationQueueArc init
- `merge_reader.rs` - MergeReader init/drop
- `pipe.rs` - Writer init/drop, Reader init/drop
- `scheduler.rs` - Scheduler init

## Test Run Analysis (5-second timeout)

### Services with BOTH init AND shutdown traces

| Service | Init Trace | Shutdown Trace |
|---------|------------|----------------|
| BlockingActorRuntime | `BlockingActorRuntime::new: creating` | `BlockingActorRuntime::shutdown: destroying` |
| Reader (pipe) | `Reader::new: creating` | `Reader: destroying (drop)` |
| attach_to_stdout | `entering read loop` | `exited read loop` |

### Services with ONLY init traces (no shutdown seen)

| Service | Init Trace | Notes |
|---------|------------|-------|
| FlushCoordinator | `FlushCoordinator::new: creating` | Still processing requests |
| FlushCoordinator::writer_loop | `entering request_rx loop` | Still in loop |
| SystemRuntime | `SystemRuntime::new: creating` + `entering request_rx loop` | Still in loop |
| Environment | `Environment::new: creating` | Consumed by run() |
| IdGen | `IdGen::new: creating` | Lives in Arc until end |
| Dag | `Dag::new: creating` | No drop impl |
| ActorRegistry | `ActorRegistry::new: creating` | No drop impl |
| NotificationQueueArc | `NotificationQueueArc::new: creating` | No drop impl |
| PipePool | `PipePool::new: creating` | No drop impl |
| AttachmentManager | `AttachmentManager::new: creating` | No drop impl |
| Scheduler | `Scheduler::new: creating` | Short-lived |
| MergeReader | `MergeReader::new: creating` | Drop exists but not seen |
| Writer (pipe) | `Writer::new: creating` | Drop exists but not seen |
| FdTable | `FdTable::new: creating` | No drop impl |

## Observations from 5-second Test Run

The program **did not hang** during the 5-second test:
- Created actors 1-5
- Processed data through pipeline
- `stdout attachment finished` appeared
- `actor shutdown complete` for actor 1
- FlushCoordinator processed 5 flush requests

The main rx loops (`SystemRuntime::run`, `FlushCoordinator::writer_loop`) were still active when timeout killed the process.

## Next Steps for Debugging

1. Run without timeout to see if it completes or hangs
2. If it hangs, check which rx loop is stuck:
   - Look for `entering request_rx loop` without corresponding `exited request_rx loop`
   - Look for `awaiting notify` without `notified, looping back`
3. Check if all actors complete shutdown (all should show `BlockingActorRuntime::shutdown: destroying`)
4. Verify all Writers are dropped (should show `Writer: destroying (drop)`)

## Key Trace Patterns to Watch

```
# Service lifecycle
"::new: creating" -> should eventually see "destroying" or "drop"

# Loops that could hang
"entering request_rx loop" -> should see "exited request_rx loop"
"entering read loop" -> should see "exited read loop"
"awaiting notify" -> should see "notified, looping back"
```
