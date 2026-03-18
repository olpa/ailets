# Pipe Pool: Producer-Consumer Coordination

## lazy-creation

Pipes are created on-demand when first accessed, not pre-allocated for all actor streams.

## idempotent-access

Multiple requests for same stream return same pipe instance.

## early-reader

Reader can open stream before producer exists. Read blocks until producer writes or terminates.

## late-reader

Reader can open stream after producer started. Reader receives full stream from the beginning.

## post-termination-reader

Reader can open stream after producer is closed, even if producing actor terminated. Reader receives full stream from the beginning.

## multiple-readers

Multiple readers can open same stream. All receive identical data sequence.

## attachments

Pool must support attaching additional readers to any stream at any time. Same timing rules apply (early, late, post-termination). Example: host output forwarding.

## cleanup-on-actor-termination

When actor terminates, pool closes all its writers. Readers receive EOF.
