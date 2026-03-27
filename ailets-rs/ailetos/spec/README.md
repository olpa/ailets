# TLA+ Specification for PipePool

This directory contains TLA+ formal verification for the pipe pool race conditions.

## Quick Start

```bash
# TLA+ tools are already installed (tla2tools.jar)

# Easy way: Use the helper script
./run_tlc.sh

# Manual way: Run TLC directly
java -XX:+UseParallelGC -cp tla2tools.jar tlc2.TLC -workers auto -config PipePool.cfg PipePool.tla

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
- `tla2tools.jar` - TLA+ model checker (v1.8.0)
- `run_tlc.sh` - Helper script to run TLC
- `README.md` - This file

## Resources

- [Learn TLA+](https://learntlaplus.com/)
- [TLA+ Hyperbook](https://www.learntla.com/)
- [TLA+ GitHub](https://github.com/tlaplus/tlaplus)
