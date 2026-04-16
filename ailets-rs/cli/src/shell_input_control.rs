//! Control registry for shell_input actors
//!
//! Uses std::sync::mpsc channels: the CLI holds the sender, the actor takes the
//! receiver. Sending data enqueues it; dropping the sender signals EOF.

use std::collections::HashMap;
use std::sync::{mpsc, Mutex};

use ailetos::Handle;
use once_cell::sync::Lazy;

static SENDERS: Lazy<Mutex<HashMap<Handle, mpsc::Sender<Vec<u8>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

static RECEIVERS: Lazy<Mutex<HashMap<Handle, mpsc::Receiver<Vec<u8>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Register a new shell_input actor. Must be called before the actor starts.
pub fn register_shell_input_actor(handle: Handle) {
    let (tx, rx) = mpsc::channel();
    SENDERS.lock().unwrap().insert(handle, tx);
    RECEIVERS.lock().unwrap().insert(handle, rx);
}

/// Take the receiver for a shell_input actor. Called once by the actor at startup.
///
/// Returns `None` if the actor was not registered or was already taken.
pub fn take_receiver(handle: Handle) -> Option<mpsc::Receiver<Vec<u8>>> {
    RECEIVERS.lock().unwrap().remove(&handle)
}

/// Enqueue data to be written by the actor.
pub fn write_to_shell_input(handle: Handle, data: Vec<u8>) -> Result<(), String> {
    let senders = SENDERS.lock().unwrap();
    match senders.get(&handle) {
        Some(tx) => tx.send(data).map_err(|_| format!("shell_input actor {:?} has already closed", handle)),
        None => Err(format!("shell_input actor {:?} not found", handle)),
    }
}

/// Close the actor's input (EOF). Drops the sender so the actor's recv() returns Err.
pub fn close_shell_input(handle: Handle) -> Result<(), String> {
    match SENDERS.lock().unwrap().remove(&handle) {
        Some(_) => Ok(()),
        None => Err(format!("shell_input actor {:?} not found", handle)),
    }
}
