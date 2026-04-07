# Erlang-Inspired Actor Interface for dagsh

## Philosophy

In Erlang, all processes are equal peers. There is no privileged "foreground" process - any process can be inspected, suspended, resumed, or terminated. The dagsh interface follows this model: actors are independent concurrent entities that can be individually controlled.

## Actor States

| State | Description |
|-------|-------------|
| `running` | Actor is actively executing |
| `suspended` | Actor is paused, will not execute until resumed |
| `waiting` | Actor is blocked on input or dependency |
| `completed` | Actor has finished execution |
| `failed` | Actor terminated with an error |

### Erlang Runtime Process States (Reference)

| State | Description |
|-------|-------------|
| `running` | Currently executing on a scheduler |
| `runnable` | Ready to run, waiting for scheduler time |
| `waiting` | Blocked in receive, waiting for a message |
| `suspended` | Explicitly suspended via `erlang:suspend_process/1` |
| `exiting` | In the process of terminating |
| `garbage_collecting` | Performing garbage collection |

Note: In Erlang, terminated processes cease to exist. Termination is reported to linked/monitoring processes via exit reasons (`normal`, `kill`, `{error, Reason}`, etc.).

## Commands

### Inspection

| Command | Erlang Equivalent | Description |
|---------|-------------------|-------------|
| `i()` | `i()` | List all actors with their states |
| `i(Id)` | `process_info(Pid)` | Detailed information about an actor |
| `regs()` | `registered()` | List actors by their registered names |
| `whereis(Name)` | `whereis(Name)` | Find actor ID by name |

### Control

| Command | Erlang Equivalent | Description |
|---------|-------------------|-------------|
| `suspend(Id)` | `erlang:suspend_process/1` | Pause an actor |
| `resume(Id)` | `erlang:resume_process/1` | Resume a suspended actor |
| `exit(Id)` | `exit(Pid, kill)` | Terminate an actor |
| `exit(Id, Reason)` | `exit(Pid, Reason)` | Terminate with a specific reason |

### Communication

| Command | Erlang Equivalent | Description |
|---------|-------------------|-------------|
| `send(Id, Data)` | `Pid ! Msg` | Send data to an actor's input |
| `flush(Id)` | `flush()` | Display and clear an actor's pending output |

### Observation

| Command | Erlang Equivalent | Description |
|---------|-------------------|-------------|
| `attach(Id)` | (remote shell) | Attach to an actor's output stream |
| `detach()` | (Ctrl+G, then c) | Detach from current actor stream |
| `trace(Id)` | `dbg:p(Pid, ...)` | Enable tracing for an actor |
| `untrace(Id)` | `dbg:p(Pid, clear)` | Disable tracing |

## Example Session

```
dagsh> i()
Id    Name           State      Dependencies
--    ----           -----      ------------
1     llm_request    running    []
2     json_parser    waiting    [1]
3     shell_input    suspended  []

dagsh> i(1)
Actor: 1
  Name: llm_request
  State: running
  Started: 2024-01-15 10:23:45
  Dependencies: []
  Dependents: [2]
  Messages in queue: 0

dagsh> suspend(1)
ok

dagsh> i()
Id    Name           State      Dependencies
--    ----           -----      ------------
1     llm_request    suspended  []
2     json_parser    waiting    [1]
3     shell_input    suspended  []

dagsh> resume(1)
ok

dagsh> attach(2)
[attached to json_parser]
{"response": "Hello...
[Ctrl+D to detach]

dagsh> send(3, "user input here")
ok

dagsh> exit(1, timeout)
ok
```

## Differences from Erlang

| Aspect | Erlang | dagsh |
|--------|--------|-------|
| Identity | PIDs (opaque) | Integer IDs or names |
| Creation | `spawn/1,3` | Actors created by DAG definition |
| Linking | `link/1`, `monitor/2` | Implicit via DAG dependencies |
| Messages | Async mailbox | Streaming data flow |

## Ctrl+G Mode (Optional)

For users familiar with Erlang's job control, an optional Ctrl+G mode:

```
dagsh> [Ctrl+G]
Actor Control
 --> j        list actors
     s Id     suspend actor
     r Id     resume actor
     k Id     kill actor
     a Id     attach to actor
     c        return to shell
```

## Design Rationale

1. **No foreground/background distinction**: All actors are peers. "Attaching" to an actor observes its output but doesn't change execution priority.

2. **Explicit state control**: `suspend`/`resume` rather than Unix signals. The intent is clear and the operation is synchronous.

3. **Named actors**: Like Erlang's registered processes, actors can have human-readable names alongside numeric IDs.

4. **Dependency-aware**: Unlike Erlang processes which are fully independent, DAG actors have explicit dependencies. Suspending an actor may cause dependents to block.
