//! Notification Queue
//!
//! Thread-safe notification mechanism for coordinating between sync workers
//! and async clients.
//!
//! # 1) Waiting for a handle
//!
//! In the first approximation, the workflow is as follows:
//!
//! 10. Client: check condition
//! 20. Client: call `wait_for_handle`
//! 30. Queue-for-client: add client to the waiting list
//! 40. Queue-for-client: wait for handle notification
//!
//! 50. Worker: call `notify`
//! 60. Queue-for-worker: extract the client(s) from the waiting list
//! 70. Queue-for-worker: notify the event loop to awake the client(s)
//!
//! 80. Queue-for-client: awake and exit from `wait_for_handle`
//!
//! However, due to the worker being in a different thread,
//! the step 60 "extract the client(s) from the waiting list" can happen
//! before the step 30 "add client to the waiting list". This way, the client
//! will not be notified about the handle event and will wait indefinitely.
//!
//! To avoid this, the client should acquire the lock to make the steps 10-30 atomic.
//!
//! To hold the lock as little as possible, here is the suggested client workflow:
//!
//! ```ignore
//! if should_wait() {
//!     do_something_preliminary();
//! }
//!
//! let lock = queue.get_lock();
//! if should_wait() {
//!     queue.wait_async(handle, debug_hint, lock).await;
//!     // Note: lock is consumed by wait_async and released before awaiting
//! }
//! ```
//!
//! # 2) Subscribing to a handle
//!
//! Nothing special here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ============================================================================
// Handle Type
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle {
    id: u64,
}

impl Handle {
    pub fn new(id: u64) -> Self {
        Self { id }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

// ============================================================================
// Client Types
// ============================================================================

/// Represents a client waiting for a handle notification
struct WaitingClient {
    /// Thread-safe sender that can be used to notify waiting clients from any thread.
    ///
    /// Proof: tokio::sync::oneshot::Sender<T> implements Send where T: Send.
    /// Since Handle is Copy (containing only u64), Sender<Handle> is Send.
    ///
    /// Documentation: <https://docs.rs/tokio/latest/tokio/sync/oneshot/struct.Sender.html>
    /// To verify: Scroll down to "Trait Implementations" section to see:
    /// - `impl<T> Send for Sender<T> where T: Send`
    /// - `impl<T> Sync for Sender<T> where T: Send`
    sender: tokio::sync::oneshot::Sender<Handle>,
    debug_hint: String,
}

impl std::fmt::Debug for WaitingClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WaitingClient")
            .field("debug_hint", &self.debug_hint)
            .finish_non_exhaustive()
    }
}

/// Broadcast channel for a handle (one channel per handle, multiple subscribers)
struct BroadcastChannel {
    sender: tokio::sync::broadcast::Sender<Handle>,
    debug_hint: String,
}

impl std::fmt::Debug for BroadcastChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BroadcastChannel")
            .field("debug_hint", &self.debug_hint)
            .field("subscriber_count", &self.sender.receiver_count())
            .finish()
    }
}

// ============================================================================
// Internal State
// ============================================================================

pub struct InnerState {
    whitelist: HashMap<Handle, String>,
    waiting_clients: HashMap<Handle, Vec<WaitingClient>>,
    broadcast_channels: HashMap<Handle, BroadcastChannel>,
}

impl InnerState {
    fn new() -> Self {
        Self {
            whitelist: HashMap::new(),
            waiting_clients: HashMap::new(),
            broadcast_channels: HashMap::new(),
        }
    }
}

// ============================================================================
// Main Queue
// ============================================================================

/// Thread-safe queue for handle (as integers) notifications
#[derive(Clone)]
pub struct NotificationQueueArc {
    inner: Arc<Mutex<InnerState>>,
}

impl NotificationQueueArc {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerState::new())),
        }
    }

    /// Get the lock for atomic condition-check + register operations
    pub fn get_lock(&self) -> std::sync::MutexGuard<'_, InnerState> {
        self.inner.lock().unwrap()
    }

    /// Register a handle in the whitelist
    pub fn whitelist(&self, handle: Handle, debug_hint: &str) {
        let mut state = self.inner.lock().unwrap();
        if let Some(old_hint) = state.whitelist.insert(handle, debug_hint.to_string()) {
            log::warn!("queue.whitelist: handle {:?} already in whitelist (was: '{}')", handle, old_hint);
        }
    }

    /// Unregister a handle from the whitelist
    pub fn unlist(&self, handle: Handle) {
        let mut state = self.inner.lock().unwrap();
        if state.whitelist.remove(&handle).is_none() {
            log::warn!("queue.unlist: handle {:?} not in whitelist", handle);
        }
        drop(state);

        // Notify with -1 and delete subscriptions
        self.notify_and_optionally_delete(handle, -1, true);
    }

    /// Notify waiting clients and subscribers for a handle
    pub fn notify(&self, handle: Handle, arg: i32) {
        self.notify_and_optionally_delete(handle, arg, false);
    }

    /// Wait for the handle notification
    ///
    /// Precondition: The caller should acquire the lock before calling this method.
    /// Post-condition: The lock is released after the method returns.
    ///
    /// See the module documentation for more details about the lock acquisition pattern.
    pub fn wait_async(&self, handle: Handle, debug_hint: &str, mut lock: std::sync::MutexGuard<'_, InnerState>) -> impl std::future::Future<Output = ()> + Send {
        let (tx, rx) = tokio::sync::oneshot::channel();

        // Early exit if handle not whitelisted
        if !lock.whitelist.contains_key(&handle) {
            // Don't warn: the whole idea of whitelist is to
            // avoid waiting in case of race conditions
            drop(lock);
            // Immediately resolve the future by sending any value (will be ignored)
            let _ = tx.send(Handle::new(0));
        } else {
            // Register waiter
            let client = WaitingClient {
                sender: tx,
                debug_hint: debug_hint.to_string(),
            };

            lock.waiting_clients
                .entry(handle)
                .or_default()
                .push(client);

            // Release lock before awaiting
            drop(lock);
        }

        // Wait for notification
        // Cleanup is handled by `notify_and_optionally_delete` which removes
        // all waiting clients from the map before sending notifications.
        //
        // The `rx.await` returns `Result<Handle, RecvError>`, but we ignore the result because:
        // - Normally never fails: `notify_and_optionally_delete` always sends before dropping senders
        // - If it fails, it means the sender was dropped without sending, which only
        //   happens if the entire `NotificationQueueArc` is dropped while clients are waiting
        // - If this happens, it's a catastrophic shutdown scenario caused by incorrect application
        //   cleanup order, and there's nothing meaningful we can do here anyway
        async move {
            let _ = rx.await;
        }
    }

    /// Subscribe to the handle notification
    ///
    /// Returns a broadcast Receiver. All subscribers receive all notifications.
    /// Drop the Receiver to unsubscribe (automatic cleanup).
    ///
    /// # Arguments
    /// * `handle` - The handle to subscribe to
    /// * `channel_capacity` - Capacity of the broadcast channel (only used when creating new channel)
    /// * `debug_hint` - Debug label for this channel (only used when creating new channel)
    pub fn subscribe(&self, handle: Handle, channel_capacity: usize, debug_hint: &str) -> Option<tokio::sync::broadcast::Receiver<Handle>> {
        let mut state = self.inner.lock().unwrap();

        if !state.whitelist.contains_key(&handle) {
            log::warn!("queue.subscribe: handle {:?} not in whitelist", handle);
            return None;
        }

        // Get or create broadcast channel for this handle
        let broadcast = state.broadcast_channels.entry(handle).or_insert_with(|| {
            let (tx, _rx) = tokio::sync::broadcast::channel(channel_capacity);
            BroadcastChannel {
                sender: tx,
                debug_hint: debug_hint.to_string(),
            }
        });

        // Subscribe to the broadcast channel (creates a new Receiver)
        Some(broadcast.sender.subscribe())
    }

    // No unsubscribe() needed - just drop the Receiver!

    /// Internal method to notify and optionally delete subscriptions
    fn notify_and_optionally_delete(&self, handle: Handle, arg: i32, delete_subscribed: bool) {
        let mut state = self.inner.lock().unwrap();

        let waiters = state.waiting_clients.remove(&handle).unwrap_or_default();

        log::debug!(
            "queue.notify: handle {:?}, arg={}, waiters: {}, subscribers: {}",
            handle,
            arg,
            waiters.len(),
            state.broadcast_channels.get(&handle).map(|bc| bc.sender.receiver_count()).unwrap_or(0)
        );

        // Notifications just wake subscribers; they execute later via async runtime

        for waiter in waiters {
            let _ = waiter.sender.send(handle);
        }
        if delete_subscribed {
            if let Some(bc) = state.broadcast_channels.remove(&handle) {
                let _ = bc.sender.send(handle);
            }
        } else {
            if let Some(bc) = state.broadcast_channels.get(&handle) {
                let _ = bc.sender.send(handle);
            }
        }

        drop(state);
    }
}

impl Default for NotificationQueueArc {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_basic_wait_notify() {
        let queue = NotificationQueueArc::new();
        let handle = Handle::new(1);

        queue.whitelist(handle, "test");

        let queue_clone = queue.clone();
        let waiter = tokio::spawn(async move {
            let lock = queue_clone.get_lock();
            queue_clone.wait_async(handle, "waiter", lock).await;
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        queue.notify(handle, 42);

        waiter.await.unwrap();
        queue.unlist(handle);
    }

    #[tokio::test]
    async fn test_subscription() {
        let queue = NotificationQueueArc::new();
        let handle = Handle::new(2);

        queue.whitelist(handle, "test");

        let mut rx = queue.subscribe(handle, 10, "test_subscriber").unwrap();

        queue.notify(handle, 1);
        queue.notify(handle, 2);

        // Receive notifications (subscribers receive the handle itself)
        assert_eq!(rx.recv().await.unwrap(), handle);
        assert_eq!(rx.recv().await.unwrap(), handle);

        // Drop receiver to unsubscribe (automatic)
        drop(rx);

        queue.unlist(handle);
    }

    #[tokio::test]
    async fn test_unlist_notifies_waiters() {
        let queue = NotificationQueueArc::new();
        let handle = Handle::new(3);

        queue.whitelist(handle, "test");

        let queue_clone = queue.clone();
        let waiter = tokio::spawn(async move {
            let lock = queue_clone.get_lock();
            queue_clone.wait_async(handle, "waiter", lock).await;
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        queue.unlist(handle);

        waiter.await.unwrap();
    }
}
