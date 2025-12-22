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
//!     queue.wait_unsafe(handle, debug_hint, lock).await;
//!     // Note: lock is consumed by wait_unsafe and released before awaiting
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

/// Represents a client subscribed to handle notifications
#[derive(Clone)]
struct SubscribedClient {
    id: u64,
    callback: Arc<dyn Fn(i32) + Send + Sync>,
    debug_hint: String,
}

impl std::fmt::Debug for SubscribedClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscribedClient")
            .field("id", &self.id)
            .field("debug_hint", &self.debug_hint)
            .finish_non_exhaustive()
    }
}

// ============================================================================
// Internal State
// ============================================================================

pub struct InnerState {
    whitelist: HashMap<Handle, String>,
    waiting_clients: HashMap<Handle, Vec<WaitingClient>>,
    subscribed_clients: HashMap<Handle, Vec<SubscribedClient>>,
    next_subscription_id: u64,
}

impl InnerState {
    fn new() -> Self {
        Self {
            whitelist: HashMap::new(),
            waiting_clients: HashMap::new(),
            subscribed_clients: HashMap::new(),
            next_subscription_id: 1,
        }
    }
}

// ============================================================================
// Main Queue
// ============================================================================

/// Thread-safe queue for handle (as integers) notifications
#[derive(Clone)]
pub struct NotificationQueue {
    inner: Arc<Mutex<InnerState>>,
}

impl NotificationQueue {
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
        if state.whitelist.contains_key(&handle) {
            log::warn!("queue.whitelist: handle {:?} already in whitelist", handle);
        }
        state.whitelist.insert(handle, debug_hint.to_string());
    }

    /// Unregister a handle from the whitelist
    pub fn unlist(&self, handle: Handle) {
        let mut state = self.inner.lock().unwrap();
        if !state.whitelist.contains_key(&handle) {
            log::warn!("queue.unlist: handle {:?} not in whitelist", handle);
        } else {
            state.whitelist.remove(&handle);
        }
        drop(state);

        // Notify with -1 and delete subscriptions
        self.notify_and_delete(handle, -1, true);
    }

    /// Notify waiting clients and subscribers for a handle
    pub fn notify(&self, handle: Handle, arg: i32) {
        self.notify_and_delete(handle, arg, false);
    }

    /// Wait for the handle notification
    ///
    /// Precondition: The caller should acquire the lock before calling this method.
    /// Post-condition: The lock is released after the method returns.
    ///
    /// See the module documentation for more details.
    /// The word "unsafe" in the method name hints that the caller should
    /// read the documentation.
    pub fn wait_unsafe(&self, handle: Handle, debug_hint: &str, mut lock: std::sync::MutexGuard<'_, InnerState>) -> impl std::future::Future<Output = ()> + Send {
        log::debug!("queue.wait_unsafe: {:?}", handle);

        // Check if handle is whitelisted and register waiter (synchronous, before any await)
        let rx = if !lock.whitelist.contains_key(&handle) {
            // Don't warn: the whole idea of whitelist is to
            // avoid waiting in case of race conditions
            drop(lock);
            None
        } else {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let client = WaitingClient {
                sender: tx,
                debug_hint: debug_hint.to_string(),
            };

            lock.waiting_clients
                .entry(handle)
                .or_insert_with(Vec::new)
                .push(client);

            // Release lock before awaiting
            drop(lock);
            Some(rx)
        };

        let queue = self.clone();

        // Return async block that does the waiting
        async move {
            if let Some(rx) = rx {
                // Wait for notification
                let _ = rx.await;

                // Clean up: re-acquire lock and remove ourselves from waiting list
                let mut state = queue.inner.lock().unwrap();
                if let Some(clients) = state.waiting_clients.get_mut(&handle) {
                    // Remove the client that just finished waiting
                    // Note: we can't identify by sender anymore since it was moved, so we remove all
                    // In practice, this list should be empty since notify removes all waiters
                    clients.clear();
                    if clients.is_empty() {
                        state.waiting_clients.remove(&handle);
                    }
                }
            }
        }
    }

    /// Subscribe to the handle notification
    ///
    /// Returns:
    ///     The handle id of the subscription, to unsubscribe later.
    pub fn subscribe<F>(&self, handle: Handle, callback: F, debug_hint: &str) -> Option<u64>
    where
        F: Fn(i32) + Send + Sync + 'static,
    {
        let mut state = self.inner.lock().unwrap();

        if !state.whitelist.contains_key(&handle) {
            log::warn!("queue.subscribe: handle {:?} not in whitelist", handle);
            return None;
        }

        let subscription_id = state.next_subscription_id;
        state.next_subscription_id += 1;

        let client = SubscribedClient {
            id: subscription_id,
            callback: Arc::new(callback),
            debug_hint: debug_hint.to_string(),
        };

        state
            .subscribed_clients
            .entry(handle)
            .or_insert_with(Vec::new)
            .push(client);

        Some(subscription_id)
    }

    /// Unsubscribe from handle notifications
    pub fn unsubscribe(&self, handle: Handle, subscription_id: u64) {
        let mut state = self.inner.lock().unwrap();

        if let Some(subscriptions) = state.subscribed_clients.get_mut(&handle) {
            if let Some(pos) = subscriptions.iter().position(|s| s.id == subscription_id) {
                subscriptions.remove(pos);
                if subscriptions.is_empty() {
                    state.subscribed_clients.remove(&handle);
                }
            } else {
                log::warn!(
                    "queue.unsubscribe: subscription {} for handle {:?} not found",
                    subscription_id,
                    handle
                );
            }
        } else {
            log::warn!(
                "queue.unsubscribe: handle {:?} not in subscribed clients",
                handle
            );
        }
    }

    /// Internal method to notify and optionally delete subscriptions
    fn notify_and_delete(&self, handle: Handle, arg: i32, delete_subscribed: bool) {
        let (waiters, subscribers) = {
            let mut state = self.inner.lock().unwrap();

            // Extract waiting clients
            let waiters = state.waiting_clients.remove(&handle).unwrap_or_default();

            // Extract or copy subscribed clients
            let subscribers = if delete_subscribed {
                state.subscribed_clients.remove(&handle).unwrap_or_default()
            } else {
                state
                    .subscribed_clients
                    .get(&handle)
                    .map(|v| v.clone())
                    .unwrap_or_default()
            };

            (waiters, subscribers)
        };

        log::debug!(
            "queue.notify: handle {:?}, arg={}, waiters: {}, subscribers: {}",
            handle,
            arg,
            waiters.len(),
            subscribers.len()
        );

        // Notify waiting clients
        for waiter in waiters {
            let _ = waiter.sender.send(handle);
        }

        // Notify subscribers
        for subscriber in subscribers {
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                (subscriber.callback)(arg);
            })) {
                log::error!(
                    "queue.notify: error in subscriber {}: {:?}",
                    subscriber.debug_hint,
                    e
                );
            }
        }
    }
}

impl Default for NotificationQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::time::Duration;

    #[tokio::test]
    async fn test_basic_wait_notify() {
        let queue = NotificationQueue::new();
        let handle = Handle::new(1);

        queue.whitelist(handle, "test");

        let queue_clone = queue.clone();
        let waiter = tokio::spawn(async move {
            let lock = queue_clone.get_lock();
            queue_clone.wait_unsafe(handle, "waiter", lock).await;
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        queue.notify(handle, 42);

        waiter.await.unwrap();
        queue.unlist(handle);
    }

    #[tokio::test]
    async fn test_subscription() {
        let queue = NotificationQueue::new();
        let handle = Handle::new(2);

        queue.whitelist(handle, "test");

        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let received_clone = Arc::clone(&received);

        let subscription_id = queue
            .subscribe(
                handle,
                move |arg| {
                    received_clone.lock().push(arg);
                },
                "test_subscriber",
            )
            .unwrap();

        queue.notify(handle, 1);
        queue.notify(handle, 2);

        tokio::time::sleep(Duration::from_millis(10)).await;

        assert_eq!(*received.lock(), vec![1, 2]);

        queue.unsubscribe(handle, subscription_id);
        queue.unlist(handle);
    }

    #[tokio::test]
    async fn test_unlist_notifies_waiters() {
        let queue = NotificationQueue::new();
        let handle = Handle::new(3);

        queue.whitelist(handle, "test");

        let queue_clone = queue.clone();
        let waiter = tokio::spawn(async move {
            let lock = queue_clone.get_lock();
            queue_clone.wait_unsafe(handle, "waiter", lock).await;
        });

        tokio::time::sleep(Duration::from_millis(10)).await;
        queue.unlist(handle);

        waiter.await.unwrap();
    }
}
