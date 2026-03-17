# Stream Data Transfer

## problem

Actor output streams need to transfer data from producer (actor) to consumers (other actors, monitoring, logging, attachments) with these characteristics:

1. **One-to-many delivery**: One producer writes data that multiple consumers can read independently
2. **Real-time propagation**: Consumers should receive data shortly after producer writes it
3. **Independent consumption**: Each consumer reads at its own pace without affecting others

## use-cases

### producer-writes

**Actor**: Running actor
**Goal**: Write output data to stream
**Scenario**: Actor executes `println!("Starting initialization")` or similar output operation
**Requirements**:
- Write must complete even if no consumers are reading
- Write must not block waiting for slow consumers
- Multiple threads in actor may write concurrently
- Data must become available to all active consumers

### consumer-reads-incrementally

**Actor**: Running actor
**Goal**: Read actor output as it becomes available
**Scenario**: Collector calls read() repeatedly, processing each chunk as actor produces it
**Requirements**:
- Read must return available data immediately if present
- Read must wait if no data available but producer still active
- Read must not skip or lose data
- Multiple read() calls must return sequential data without gaps

### multiple-consumers-same-stream

**Actor**: Running actors and attachments
**Goal**: Several components read same actor's output independently
**Scenario**: Dashboard displays real-time output, archiver saves to disk
**Requirements**:
- Dashboard can read at display refresh rate (e.g., 60 FPS)
- Archiver can read at disk write rate (different pace)
- Neither consumer blocks the other
- Both receive identical data sequence
- Each maintains independent read position

### producer-closes

**Actor**: Any stream consumer
**Goal**: Detect when producer finishes writing
**Scenario**: Actor terminates normally or abnormally
**Requirements**:
- Consumer's read must eventually return EOF
- EOF must occur after all written data has been read
- Consumer must not miss final data before EOF

### slow-consumer

**Actor**: Slow consumer (e.g., writing to network storage)
**Goal**: Read all data without causing backpressure on producer
**Context**: Producer writes quickly, consumer processes slowly
**Requirement**: System must handle speed mismatch without:
- Blocking producer
- Losing data
- Consuming unbounded memory

### late-joiner

**Actor**: Monitoring component starting after actor is running
**Goal**: Read output from point of subscription onward
**Scenario**: Actor has been running for 30 seconds. Monitor subscribes at T=30s.
**Requirement**: Monitor should receive data written from T=30s onward, not historical data from T=0..T=30s.

## requirements

### write-semantics

Write operation must:
- Accept byte buffer and length
- Return number of bytes written, or error indicator
- Complete in bounded time (not block indefinitely)
- Be atomic: data from one write must not interleave with data from concurrent writes
- Support concurrent writes from multiple threads

### read-semantics

Read operation must:
- Accept buffer to receive data
- Return number of bytes read, EOF indicator, or error
- Wait if no data available but producer still active
- Return EOF if producer has closed and all data consumed
- Support concurrent reads by independent reader instances

### posix-like-behavior

To minimize surprises for developers familiar with UNIX pipes:
- Write returns positive number (bytes written), 0 (special case), or -1 (error)
- Read returns positive number (bytes read), 0 (EOF), or -1 (error)
- Empty writes (zero-length) should be allowed but may be treated specially

### independent-reader-positions

Each reader maintains its own:
- Current read position
- EOF status
- Error state

One reader advancing its position must not affect other readers' positions.

### memory-bounded

System should not accumulate unbounded data in memory.

Note: This spec does not prescribe mechanism (e.g., circular buffer, eviction policy), but any implementation must address this.

### wait-notification

When reader waits for data:
- Reader must not busy-wait (spin loop)
- Reader must be notified when data becomes available
- Reader must be notified when writer closes
- Notification mechanism must not create deadlocks

### broadcast-consistency

All readers must observe the same sequence of bytes in the same order. If producer writes "ABC" then "DEF", every reader must see "ABCDEF", never "ADBECF" or "DEF" only.
