//! Control registry for `shell_input` actors
//!
//! Uses `std::sync::mpsc` channels: the CLI holds the sender, the actor takes the
//! receiver. Sending data enqueues it; dropping the sender signals EOF.

use std::collections::HashMap;
use std::sync::{mpsc, LazyLock};

use ailetos::Handle;
use parking_lot::Mutex;

static SENDERS: LazyLock<Mutex<HashMap<Handle, mpsc::Sender<Vec<u8>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static RECEIVERS: LazyLock<Mutex<HashMap<Handle, mpsc::Receiver<Vec<u8>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Register a new `shell_input` actor. Must be called before the actor starts.
pub fn register_shell_input_actor(handle: Handle) {
    let (tx, rx) = mpsc::channel();
    SENDERS.lock().insert(handle, tx);
    RECEIVERS.lock().insert(handle, rx);
}

/// Take the receiver for a `shell_input` actor. Called once by the actor at startup.
///
/// Returns `None` if the actor was not registered or was already taken.
pub fn take_receiver(handle: Handle) -> Option<mpsc::Receiver<Vec<u8>>> {
    RECEIVERS.lock().remove(&handle)
}

/// Enqueue data to be written by the actor.
pub fn write_to_shell_input(handle: Handle, data: Vec<u8>) -> Result<(), String> {
    let senders = SENDERS.lock();
    match senders.get(&handle) {
        Some(tx) => tx
            .send(data)
            .map_err(|_| format!("shell_input actor {handle:?} has already closed")),
        None => Err(format!("shell_input actor {handle:?} not found")),
    }
}

/// Close all registered `shell_input` actors (EOF). Call this on shell exit so that
/// actors blocked in `for data in receiver` unblock and their `spawn_blocking` tasks
/// can complete, allowing the Tokio runtime to shut down cleanly.
pub fn close_all_shell_inputs() {
    SENDERS.lock().clear();
}

/// Close the actor's input (EOF). Drops the sender so the actor's `recv()` returns Err.
/// Also cleans up the receiver if the actor never started.
pub fn close_shell_input(handle: Handle) -> Result<(), String> {
    // Do not remove the receiver here: if the actor hasn't started yet (likely when the
    // script runs faster than the executor), removing it causes take_receiver() to return
    // None and the actor fails before processing any data. The sender drop below is enough
    // to signal EOF; the actor takes and owns the receiver itself.
    // RECEIVERS.lock().remove(&handle);
    match SENDERS.lock().remove(&handle) {
        Some(_) => Ok(()),
        None => Err(format!("shell_input actor {handle:?} not found")),
    }
}
