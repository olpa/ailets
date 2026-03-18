# Host Output Integration

## selective-stdout

Operator can specify which actors' stdout forwards to host stdout.

## logs-always-forwarded

Logs, errors, metrics, traces from all actors always forward to host stderr.

## stream-multiplexing

Forwarding works alongside internal consumers. Both can read same stream simultaneously.

## lazy-creation

Host output forwarding is established when the stream is first accessed, not pre-allocated for all actors at startup.
