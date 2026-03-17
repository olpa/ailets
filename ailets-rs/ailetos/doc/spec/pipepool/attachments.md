# Host Output Integration

## problem

When running actors that produce output, the system operator needs to:

1. **See selected output in real-time**: View stdout/logs from specific actors on the host terminal
2. **Distinguish stream types**: Route different stream types (stdout, logs, errors, traces) to appropriate host outputs
3. **Selective forwarding**: Control which actors' output appears on host terminal vs. only stored internally
4. **Real-time visibility**: See output as it's produced, not buffered with long delays

## use-cases

### single-actor-mode

**Actor**: Developer running a single actor interactively
**Goal**: See actor's output on host terminal as if running the program directly
**Scenario**:
```bash
$ ailets run my-script.py
Starting initialization...
Processing data...
Complete!
```
**Requirements**:
- Actor's stdout appears on host stdout
- Actor's stderr/logs appear on host stderr
- Output appears in real-time, not buffered
- Multiple actors' output should not be interleaved

### multi-actor-selective

**Actor**: System operator running multiple actors
**Goal**: See output from one "main" actor while others run silently
**Scenario**:
```bash
$ ailets run --attach main-actor helper-1 helper-2
[Only main-actor stdout visible on terminal]
[helper-1 and helper-2 output captured but not shown]
```
**Requirements**:
- Operator can specify which actor(s) have stdout forwarded to host
- All actors' logs still captured internally for later review
- Selective forwarding decided before actors start

### log-aggregation

**Actor**: Production system running multiple actors
**Goal**: All logs/traces go to host stderr for aggregation, regardless of which actor produces them
**Scenario**: Logs from all actors appear on host stderr with timestamps, can be redirected to log file
**Requirements**:
- Log, trace, metrics streams from all actors forwarded to host stderr
- Works even if operator hasn't explicitly configured each actor
- Consistent behavior: logs always go to stderr

### silent-operation

**Actor**: Batch processing system
**Goal**: Run actors without terminal output, only capture to internal storage
**Scenario**: System runs actors but doesn't forward any output to host terminal
**Requirement**: System must support running actors without forwarding to host streams.

### real-time-monitoring

**Actor**: DevOps engineer troubleshooting issue
**Goal**: See actor output immediately as it happens
**Scenario**: Actor writes "Checkpoint 1", engineer sees it instantly, actor hangs, engineer knows it got to checkpoint 1
**Requirement**: Output forwarding must not buffer for long periods. Engineer should see output within milliseconds.

### late-attachment

**Actor**: Operator starting to monitor already-running actor
**Goal**: Attach to running actor to see new output
**Scenario**:
1. Actor started at T=0 without attachment
2. Operator runs command to attach at T=30
3. Operator sees output from T=30 onward

**Requirement**: System should support attaching to already-running actor streams.

## requirements

### stream-routing-policy

System must have policy for which actor streams forward to which host outputs:

Minimum required stream types:
- **stdout**: Selective forwarding to host stdout (configurable per actor)
- **logs/errors**: Always forward to host stderr (all actors)
- **metrics/traces**: Always forward to host stderr (all actors)

### selective-stdout

Operator must be able to specify which actors' stdout is forwarded to host stdout.

Default behavior when not specified: implementation choice, but must be consistent and documented.

### real-time-forwarding

Forwarded output must appear on host terminal with minimal latency.

"Minimal latency" means:
- Not waiting for large buffer to fill
- Not waiting for actor to terminate
- Acceptable: per-line buffering or small chunks (< 4KB)
- Unacceptable: buffering multiple megabytes before flushing

### no-interleaving

Within a single actor's output, data must not be interleaved incorrectly.

If actor writes "ABC" then "DEF" to stdout, host must see "ABCDEF", never "ADBECF".

Note: Interleaving between different actors' output is acceptable and may be unavoidable.

### stream-multiplexing-compatibility

Host output forwarding must work alongside internal stream consumers.

Example: Log aggregator reads actor's log stream AND logs are forwarded to host stderr. Both must work simultaneously.

### forwarding-lifecycle

Forwarding must:
- Start when actor begins producing output (or when attachment configured)
- Continue as long as actor produces output
- Stop cleanly when actor terminates
- Not block actor from terminating

### resource-cleanup

When actor terminates:
- Forwarding mechanism must stop
- Resources used for forwarding must be released
- No leaked background processes or threads

### configuration-timing

System must handle:
- **Early configuration**: Forwarding configured before actor starts
- **Just-in-time configuration**: Forwarding configured as actor starts
- **Late attachment** (optional): Forwarding configured after actor already running

## out-of-scope

### historical-output

System does NOT need to provide historical output when attaching to running actor.

Late attachment shows output from attachment point forward, not from actor start.

### output-ordering-between-actors

System does NOT guarantee ordering of output between different actors.

If actor A writes "X" and actor B writes "Y" to stderr, they may appear as "XY" or "YX" on host stderr.

### zero-copy

Output forwarding does NOT need to be zero-copy or achieve maximum theoretical throughput.

Human-readable output is typically small (KB/s to MB/s). Optimization focus should be on latency, not raw throughput.

## invariants

### no-data-loss

Once output is written by actor and forwarding is active, that output must eventually appear on host stream (barring host stream errors).

### eof-propagation

When actor stream closes, forwarding must eventually stop reading and clean up. Must not block indefinitely waiting for more data.
