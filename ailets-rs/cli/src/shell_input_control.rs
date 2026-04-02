//! Control registry for shell_input actors
//!
//! This module provides a global registry for controlling shell_input actors that
//! receive data from shell commands and forward it to stdout.
//!
//! The shell_input actor waits for data to be enqueued via the `write` command,
//! writes it to stdout, and continues until a `close` command is issued.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};

use ailetos::Handle;
use once_cell::sync::Lazy;

/// State of a shell_input actor
enum ShellInputState {
    /// Actor is waiting for data
    Waiting,
    /// Data is available to write
    DataAvailable,
    /// Actor should close (EOF)
    Closed,
}

/// Control structure for a shell_input actor
pub struct ShellInputControl {
    state: Mutex<ShellInputState>,
    condvar: Condvar,
    /// Queue of data to write
    queue: Mutex<VecDeque<Vec<u8>>>,
}

impl ShellInputControl {
    fn new() -> Self {
        Self {
            state: Mutex::new(ShellInputState::Waiting),
            condvar: Condvar::new(),
            queue: Mutex::new(VecDeque::new()),
        }
    }

    /// Enqueue data to be written to stdout
    pub fn enqueue_data(&self, data: Vec<u8>) {
        let mut queue = self.queue.lock().unwrap();
        queue.push_back(data);

        let mut state = self.state.lock().unwrap();
        *state = ShellInputState::DataAvailable;
        self.condvar.notify_all();
    }

    /// Signal that no more data will be written (EOF)
    pub fn close(&self) {
        let mut state = self.state.lock().unwrap();
        *state = ShellInputState::Closed;
        self.condvar.notify_all();
    }

    /// Wait for data or close signal, returns Some(data) if data available, None if closed
    pub fn wait_for_data(&self) -> Option<Vec<u8>> {
        let mut state = self.state.lock().unwrap();

        loop {
            match *state {
                ShellInputState::DataAvailable => {
                    let mut queue = self.queue.lock().unwrap();
                    if let Some(data) = queue.pop_front() {
                        if queue.is_empty() {
                            *state = ShellInputState::Waiting;
                        }
                        return Some(data);
                    }
                    // Queue is empty, go back to waiting
                    *state = ShellInputState::Waiting;
                }
                ShellInputState::Closed => {
                    return None;
                }
                ShellInputState::Waiting => {
                    state = self.condvar.wait(state).unwrap();
                }
            }
        }
    }
}

/// Global registry of shell_input actor controls indexed by node handle
static REGISTRY: Lazy<Mutex<HashMap<Handle, Arc<ShellInputControl>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Register a new shell_input actor
pub fn register_shell_input_actor(handle: Handle) -> Arc<ShellInputControl> {
    let mut registry = REGISTRY.lock().unwrap();
    let control = Arc::new(ShellInputControl::new());
    registry.insert(handle, Arc::clone(&control));
    control
}

/// Get the shell_input control for a specific actor by its node handle
pub fn get_shell_input_control(handle: Handle) -> Option<Arc<ShellInputControl>> {
    let registry = REGISTRY.lock().unwrap();
    registry.get(&handle).cloned()
}

/// Write data to a shell_input actor
pub fn write_to_shell_input(handle: Handle, data: Vec<u8>) -> Result<(), String> {
    let registry = REGISTRY.lock().unwrap();
    if let Some(control) = registry.get(&handle) {
        control.enqueue_data(data);
        Ok(())
    } else {
        Err(format!("shell_input actor with handle {:?} not found", handle))
    }
}

/// Close a shell_input actor (send EOF)
pub fn close_shell_input(handle: Handle) -> Result<(), String> {
    let registry = REGISTRY.lock().unwrap();
    if let Some(control) = registry.get(&handle) {
        control.close();
        Ok(())
    } else {
        Err(format!("shell_input actor with handle {:?} not found", handle))
    }
}
