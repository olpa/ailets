# TLA+ Specification for PipePool

This directory contains TLA+ formal verification for the pipe pool race conditions.

## Quick Start

```bash
# Install TLA+ tools
wget https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar

# Run model checker
java -XX:+UseParallelGC -cp tla2tools.jar tlc2.TLC PipePool.tla

# Or use TLA+ Toolbox (GUI)
# Download from: https://github.com/tlaplus/tlaplus/releases
```

## Expected Results

This spec models the **buggy code** (commit 5ac954c) before the race condition fixes.

**TLC should find violations:**
- `NoCoexistence` - Race #2: Writer and latent coexist
- `NoDuplicateLatents` - Race #4: Multiple latents for same key

See `../tla_experiment_handover.md` for full details.

## Files

- `PipePool.tla` - Main specification
- `PipePool.cfg` - Model configuration
- `README.md` - This file

## Resources

- [Learn TLA+](https://learntlaplus.com/)
- [TLA+ Hyperbook](https://www.learntla.com/)
- [TLA+ GitHub](https://github.com/tlaplus/tlaplus)
