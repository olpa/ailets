# Stream Data Transfer

## problem

Actor output streams need to transfer data from producer (actor) to consumers (other actors, monitoring, logging, attachments) with these characteristics:

1. **One-to-many delivery**: One producer writes data that multiple consumers can read independently
2. **Real-time propagation**: Consumers should receive data shortly after producer writes it
3. **Independent consumption**: Each consumer reads at its own pace without affecting others

## producer-writes

**Actor**: Running actor
**Goal**: Write output data to stream
**Scenario**: Actor executes `println!("Starting initialization")` or similar output operation
**Requirements**:
- Write must complete even if no consumers are reading
- Write must not block waiting for slow consumers
- Multiple threads in actor may write concurrently
- Data must become available to all active consumers

## consumer-reads-incrementally

**Actor**: Running actor
**Goal**: Read actor output as it becomes available
**Scenario**: Collector calls read() repeatedly, processing each chunk as actor produces it
**Requirements**:
- Read must return available data immediately if present
- Read must wait if no data available but producer still active
- Read must not skip or lose data
- Multiple read() calls must return sequential data without gaps

## multiple-independent-consumers

**Actor**: Multiple components reading from same stream
**Goal**: Several components read same actor's output independently
**Scenarios**:
1. Value node provides data to multiple downstream actors
2. Actor's stdout goes both to next actor in pipeline AND to host stdout (attachment)
3. Actor's logs go to log aggregator AND to host stderr

**Requirements**:
- Each consumer receives identical data sequence
- Each consumer maintains independent read position
- Consumers read at different speeds without blocking each other
- System handles speed mismatches without:
  - Blocking the producer
  - Blocking fast consumers waiting for slow ones
  - Losing data

## producer-closes

**Actor**: Any stream consumer
**Goal**: Detect when producer finishes writing
**Scenario**: Actor terminates normally or abnormally
**Requirements**:
- Consumer's read must eventually return EOF
- EOF must occur after all written data has been read
- Consumer must not miss final data before EOF

## late-joiner

**Actor**: Component subscribing to already-running actor's stream
**Goal**: Read output from the current buffer state
**Scenario**: Actor starts at T=0 and begins writing. Consumer subscribes at T=30s.
**Requirement**: Consumer receives all data currently in the buffer, then continues reading new data as it arrives.