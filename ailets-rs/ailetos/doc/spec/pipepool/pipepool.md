# Actor Output Stream Multiplexing

## problem

Multiple actors run concurrently and produce output on multiple standard streams (stdout, stderr, logs, metrics, traces). The system must:

1. **Isolate streams**: Each actor's stdout must be separate from every other actor's stdout. Similarly for other stream types.
2. **Support multiple consumers**: Any component should be able to read an actor's output, even if other components are already reading it.
3. **Handle timing uncertainties**: Consumers may start reading before or after the producer starts writing.
4. **Prevent resource leaks**: When an actor terminates, all its output streams must be properly closed.

## use-cases

### read-actor-output

**Actor**: System component (e.g., attachment manager, log collector, monitoring dashboard)
**Goal**: Read output from a specific actor's stream
**Constraints**:
- Must not interfere with other consumers reading the same stream
- Must receive all data written after starting to read
- Must detect when the actor stops producing output (EOF)

### actor-produces-output

**Actor**: Running actor
**Goal**: Write data to one of its output streams
**Constraints**:
- Writing must succeed whether or not anyone is reading
- Multiple threads within the actor may write concurrently
- Writes must be atomic and not interleaved

### late-consumer

**Actor**: Monitoring component
**Goal**: Start reading an actor's output after the actor has already been running
**Scenario**: Actor A starts at T=0 and begins producing logs. Monitor B starts at T=10 and wants to read all logs produced from T=10 onward (not historical logs from T=0..T=10).
**Requirement**: Consumer must be able to attach to a running stream.

### early-consumer

**Actor**: Log collector
**Goal**: Start reading an actor's output before the actor starts producing
**Scenario**: Collector wants to capture all output from actor initialization, so it opens the read stream before the actor calls its first write.
**Requirement**: Read operation must wait until data is available, not fail immediately.

### actor-never-writes

**Actor**: Log collector
**Goal**: Handle case where actor terminates without ever writing to a stream
**Scenario**: Collector opens read on actor's stdout expecting output. Actor runs and terminates successfully but never wrote to stdout.
**Requirement**: Read operation must eventually return EOF (not block forever) when actor terminates.

### actor-crashes

**Actor**: Monitoring dashboard
**Goal**: Detect when monitored actor terminates abnormally
**Requirement**: When actor crashes or is killed, all its output streams must signal EOF to readers, not leave readers blocked forever.

## requirements

### stream-identity

Each combination of (actor identifier, stream type) represents a unique logical output stream.

Stream types include at minimum: stdout, stderr, log, metrics, trace.

### isolation

Output from different actors must never be mixed, even if they write to similarly-named streams.

### broadcast-semantics

A single actor output stream must support multiple concurrent readers, each receiving an independent copy of the data.

### reader-independence

Each reader maintains its own:
- Read position in the stream
- EOF status
- Error state

One reader closing its view of the stream must not affect other readers.

### lazy-resource-allocation

Creating a reader or writer for a stream must not pre-allocate resources for streams that are never used.

If an actor never writes to stdout, no memory should be allocated for its stdout stream.

### race-free-startup

When consumer requests stream before producer starts:
- Consumer's request must not fail
- Consumer must receive all data that producer subsequently writes
- Consumer must not miss initial output due to timing

### guaranteed-eof

When an actor terminates:
- All readers of that actor's streams must eventually receive EOF
- No reader may block forever waiting for data
- This must hold even if actor never wrote to some streams

### thread-safety

Multiple threads may concurrently:
- Write to the same stream (from same actor)
- Read from the same logical stream (different reader instances)
- Create readers for different streams

All operations must be safe without data races or corruption.

### minimal-blocking

Reader blocking must be proportional to actual write operations, not to number of other readers or internal coordination overhead.
