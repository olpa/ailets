# Refactoring Advice: dbg_actor and shell_input_actor

## Overview

This document analyzes `dbg_actor` and `shell_input_actor` to determine if they can be unified, and provides recommendations for potential refactoring.

## Actor Comparison

### dbg_actor

**Purpose**: Pass-through debugging actor that reads from stdin, optionally pauses after N bytes, waits for a resume signal, then continues forwarding data to stdout.

**Characteristics**:
- Position in pipeline: Middle node (filter)
- Data source: stdin (upstream pipeline node)
- Data flow: Push model - data flows through it
- Control signal: `resume`
- Termination: EOF from stdin

**Use cases**:
- Testing on-demand actor spawning
- Debugging pipeline flow
- Controlled pause points for inspection

### shell_input_actor

**Purpose**: Source actor that receives data via shell commands (`write`) and forwards to stdout until `close` is called.

**Characteristics**:
- Position in pipeline: Source node (start of chain)
- Data source: Shell commands via internal queue
- Data flow: Pull model - waits for externally enqueued data
- Control signals: `write`, `close`
- Termination: `close` command

**Use cases**:
- Interactive shell integration
- Background job support
- On-demand data injection into running pipelines

## Architectural Similarities

Both actors share common infrastructure patterns:

1. **Global registry pattern**: `Lazy<Mutex<HashMap<Handle, Arc<Control>>>>`
2. **Condvar-based synchronization**: Mutex + Condvar for thread-safe state management
3. **Control structure pattern**: Arc-wrapped control for shared ownership
4. **Error handling**: Both use `error_kind_to_str()` for readable errors
5. **Registration flow**: Register at node creation, lookup at execution

## Can They Be United?

**Recommendation: No, keep them separate.**

### Reasons

1. **Fundamentally different roles**:
   - `dbg_actor` is a *filter* (transforms/forwards existing data)
   - `shell_input_actor` is a *source* (generates data for the pipeline)

2. **Different pipeline positions**:
   - `shell_input_actor` has no stdin input - it's always at the start
   - `dbg_actor` requires stdin - it's always in the middle

3. **Different semantics**:
   - `dbg_actor`: "pause flow of existing data"
   - `shell_input_actor`: "inject new data into pipeline"

4. **Single responsibility principle**: Each actor has a clear, focused purpose. A combined actor would need complex mode switching and confuse users.

5. **Usage patterns don't overlap**: You wouldn't use them interchangeably in real scenarios.

## Recommended Refactoring

Instead of unifying the actors, extract shared infrastructure:

### 1. Generic Actor Control Registry

Create a reusable registry abstraction:

```rust
// src/actor_control_registry.rs

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

pub struct ActorControlRegistry<T> {
    controls: Mutex<HashMap<Handle, Arc<T>>>,
}

impl<T> ActorControlRegistry<T> {
    pub fn new() -> Self {
        Self {
            controls: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, handle: Handle, control: T) {
        let mut controls = self.controls.lock().unwrap();
        controls.insert(handle, Arc::new(control));
    }

    pub fn get(&self, handle: Handle) -> Option<Arc<T>> {
        let controls = self.controls.lock().unwrap();
        controls.get(&handle).cloned()
    }

    pub fn list(&self) -> Vec<Handle> {
        let controls = self.controls.lock().unwrap();
        controls.keys().cloned().collect()
    }
}
```

### 2. Waitable State Utility

Extract the condvar-based waiting pattern:

```rust
// src/waitable_state.rs

use std::sync::{Condvar, Mutex};

pub struct WaitableState<T> {
    state: Mutex<T>,
    condvar: Condvar,
}

impl<T> WaitableState<T> {
    pub fn new(initial: T) -> Self {
        Self {
            state: Mutex::new(initial),
            condvar: Condvar::new(),
        }
    }

    pub fn wait_until<F>(&self, condition: F) -> T
    where
        F: Fn(&T) -> bool,
        T: Clone,
    {
        let guard = self.state.lock().unwrap();
        let guard = self.condvar.wait_while(guard, |s| !condition(s)).unwrap();
        guard.clone()
    }

    pub fn update<F>(&self, updater: F)
    where
        F: FnOnce(&mut T),
    {
        let mut guard = self.state.lock().unwrap();
        updater(&mut *guard);
        self.condvar.notify_all();
    }
}
```

### 3. Refactored Structure

```
src/
├── actor_control_registry.rs    # Generic registry (NEW)
├── waitable_state.rs            # Condvar utility (NEW)
├── dbg_actor.rs                 # Uses shared utilities
├── dbg_control.rs               # Uses ActorControlRegistry<DbgControl>
├── shell_input_actor.rs         # Uses shared utilities
└── shell_input_control.rs       # Uses ActorControlRegistry<ShellInputControl>
```

## Benefits of This Approach

1. **Reduced code duplication**: Registry and synchronization logic written once
2. **Consistent patterns**: All future actors use the same infrastructure
3. **Clear separation**: Each actor retains its focused purpose
4. **Easier testing**: Shared utilities can be tested independently
5. **Better maintainability**: Bug fixes in shared code benefit all actors

## Migration Path

1. Create `actor_control_registry.rs` with generic implementation
2. Create `waitable_state.rs` with condvar abstraction
3. Refactor `dbg_control.rs` to use `ActorControlRegistry<DbgControl>`
4. Refactor `shell_input_control.rs` to use `ActorControlRegistry<ShellInputControl>`
5. Update actors to use `WaitableState` where appropriate
6. Add tests for shared utilities

## Conclusion

While `dbg_actor` and `shell_input_actor` cannot be meaningfully unified due to their fundamentally different purposes, there is significant opportunity to reduce code duplication by extracting shared infrastructure patterns into reusable utilities.
