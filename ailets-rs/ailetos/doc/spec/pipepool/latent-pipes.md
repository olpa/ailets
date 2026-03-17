# Producer-Consumer Timing Independence

## problem

In a multi-actor system, components need to read actor output but cannot control timing:

1. **Consumer starts first**: A log collector wants to capture all output from actor initialization, so it tries to open the stream before the actor has written anything
2. **Producer starts first**: An actor begins producing output before any monitoring component has subscribed
3. **Producer never writes**: An actor runs successfully but never writes to a particular stream (e.g., stdout)
4. **Producer terminates early**: An actor crashes before a monitoring component subscribes

Without coordination, these timing scenarios create race conditions where consumers either block forever or miss the producer's lifecycle events.

## use-cases

### capture-initialization

**Actor**: Log archiving system
**Goal**: Capture complete actor lifecycle including initialization messages
**Scenario**:
1. System prepares to start actor
2. Archiver subscribes to actor's log stream
3. Actor starts and writes "Initializing..."
4. Archiver receives the message

**Requirement**: Subscription must succeed before producer exists, and subscriber must receive all subsequent output.

### monitor-running-actor

**Actor**: Runtime monitoring dashboard
**Goal**: Attach to already-running actor to see current activity
**Scenario**:
1. Actor starts at T=0, writes to logs
2. User opens dashboard at T=60
3. Dashboard subscribes to actor's log stream at T=60
4. Actor continues writing, dashboard displays new messages from T=60 onward

**Requirement**: Subscription to running actor must succeed and deliver future output.

### silent-stream

**Actor**: Process that may or may not produce output
**Goal**: System should handle actors that never write to certain streams
**Scenario**:
1. Log collector subscribes to actor's stdout
2. Actor runs for 10 seconds performing work
3. Actor terminates successfully without ever writing to stdout
4. Collector's read operation must return EOF, not block forever

**Requirement**: When producer terminates without ever writing, consumers waiting on that stream must receive EOF.

### crash-before-first-write

**Actor**: Monitoring system tracking actor health
**Goal**: Detect actor failure even if actor crashes before producing output
**Scenario**:
1. Monitor subscribes to actor's stdout to detect "Ready" message
2. Actor crashes during initialization before writing anything
3. Monitor's read must return EOF, not block forever waiting for "Ready"

**Requirement**: Actor termination must unblock consumers even if stream was never written to.

### multiple-waiters

**Actor**: System with multiple monitoring components
**Goal**: Multiple components subscribe to stream before actor starts
**Scenario**:
1. Log archiver subscribes to actor's stdout (not started yet)
2. Real-time monitor subscribes to same stdout (not started yet)
3. Actor starts and writes "Hello"
4. Both archiver and monitor receive "Hello"

**Requirement**: Multiple early subscribers must all receive data when producer starts.

## requirements

### early-subscription

System must support subscribing to a stream before the producer exists.

Early subscription must:
- Not fail or return immediate EOF
- Block the subscriber until producer writes or terminates
- Deliver all data written after subscription point

### late-subscription

System must support subscribing to a stream after the producer has started writing.

Late subscription must:
- Succeed if producer is still active
- Deliver data written after subscription point (not historical data)
- Handle case where producer terminates between subscription and first read

### termination-notification

When an actor terminates, all subscribers to its streams must be notified, including:
- Streams that were written to (must deliver EOF after final data)
- Streams that were never written to (must deliver immediate EOF)
- Streams with early subscribers still waiting for first write

### no-indefinite-blocking

No reader may block forever. Every blocking read must eventually:
- Return data (if producer writes)
- Return EOF (if producer terminates)
- Return error (if exceptional condition)

### multiple-early-subscribers

Multiple consumers may subscribe to same stream before producer starts.

When producer starts writing, all early subscribers must:
- Be unblocked
- Receive identical data sequence
- Proceed independently afterward

### graceful-never-written

If an actor terminates without ever writing to a stream that has subscribers:
- All subscribers must receive EOF
- No subscriber may block forever
- System must not leak resources (waiting subscribers)

### atomic-state-transitions

Race between "consumer subscribes" and "producer starts writing" must be handled atomically.

Invalid outcomes:
- Consumer subscribes, producer starts, consumer misses initial data
- Consumer subscribes, producer terminates, consumer blocks forever

Valid outcomes:
- Consumer gets all data from producer start
- Consumer gets EOF if producer already terminated

### coordination-overhead

Coordination between early subscribers and late producers must:
- Not consume unbounded resources
- Not require polling or busy-waiting
- Use event-based notification when possible

## invariants

### no-lost-wakeups

If producer starts writing while consumer is waiting:
- Consumer must be woken
- Consumer must not miss the notification
- Consumer must observe that data is available

### termination-finality

Once an actor terminates:
- No new writers can be created for its streams
- All waiters on its streams must eventually unblock with EOF
- State transition to "terminated" must be irreversible
