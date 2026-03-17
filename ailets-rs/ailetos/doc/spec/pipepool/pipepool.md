# Pipe Pool: On-Demand Stream Management

## problem

Actors have multiple output streams (stdout, stderr, logs, metrics, traces). Creating all possible pipe objects upfront wastes memory - most actors won't write to all streams.

However, we can't simply create pipes lazily when first written to, because:
1. Consumers may want to subscribe before the actor writes anything
2. A reader may be instantiated and try to read before the writer is created

The system needs a registry that:
1. Creates pipes on-demand (only when actually needed)
2. Handles timing coordination when reader exists before writer (see [latent-pipes.md](latent-pipes.md))

## timing-coordination

The pipe pool solves timing issues using **latent pipes** - see [latent-pipes.md](latent-pipes.md) for full specification.

Brief summary:
- **Early consumer**: Consumer can subscribe before producer exists → uses latent pipe mechanism
- **Late consumer**: Consumer subscribes to already-writing producer → gets existing pipe
- **Producer never writes**: Actor terminates without writing → latent pipe resolves to EOF

## cleanup-on-actor-termination

When actor terminates, pool must:
- Close all realized pipes for that actor
- Resolve all latent pipes for that actor to EOF (see [latent-pipes.md](latent-pipes.md))
- Ensure no resources leak
