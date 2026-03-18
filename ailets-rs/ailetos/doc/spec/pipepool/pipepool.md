# Pipe Pool: On-Demand Stream Management

## lazy-creation

Pipes are created on-demand when first accessed, not pre-allocated for all actor streams.

## early-subscription

Subscribe before producer exists. Subscription blocks until producer writes or terminates.

## late-subscription

Subscribe to running producer. Subscriber receives data written after subscription point.

## termination-notification

All subscribers receive EOF when actor terminates, whether stream was written to or not.

## no-indefinite-blocking

Every read eventually returns data, EOF, or error. No reader blocks forever.

## multiple-early-subscribers

Multiple consumers can subscribe before producer exists. All receive identical data sequence when producer starts.

## atomic-state-transitions

No race between subscribe and producer start. Consumer never misses initial data or blocks forever due to timing.

## termination-finality

Once actor terminates, no new writers can be created. All waiters unblock with EOF. State transition is irreversible.

## cleanup-on-actor-termination

When actor terminates, pool closes all pipes and releases all resources. No leaks.
