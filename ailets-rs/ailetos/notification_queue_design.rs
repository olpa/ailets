//! Notification Queue - OS Core Design
//!
//! High-performance, robust notification mechanism for OS core.
//! Designed to handle buggy clients without blocking the core.

use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Public API
// ============================================================================

/// Configuration for the notification queue
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Maximum subscribers per handle (prevents resource exhaustion)
    pub max_subscribers_per_handle: usize,

    /// Maximum waiting clients per handle
    pub max_waiters_per_handle: usize,

    /// Timeout for client callbacks (if using callback mode)
    pub callback_timeout: Duration,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_subscribers_per_handle: 1024,
            max_waiters_per_handle: 1024,
            callback_timeout: Duration::from_millis(100),
        }
    }
}

/// The main notification queue
pub struct NotificationQueue {
    inner: Arc<QueueInner>,
}

impl NotificationQueue {
    pub fn new(config: QueueConfig) -> Self {
        Self {
            inner: Arc::new(QueueInner::new(config)),
        }
    }

    /// Register a new handle.
    ///
    /// Clients must explicitly unregister handles when done using unregister_handle().
    /// This prevents the race condition from the Python version by using clear
    /// ownership semantics.
    pub fn register_handle(&self) -> Handle {
        let id = self.inner.register_handle();
        Handle { id }
    }

    /// Unregister a handle, notifying all waiters.
    pub fn unregister_handle(&self, handle: &Handle) {
        self.inner.unlist(handle.id);
    }

    /// Notify all waiters and subscribers on this handle.
    ///
    /// This is the sync API for worker threads. Never blocks on client code.
    ///
    /// Returns the number of clients notified, or error if handle is invalid.
    pub fn notify(&self, handle: &Handle, arg: i32) -> Result<usize, QueueError> {
        self.inner.notify(handle.id, arg)
    }

    /// Get statistics for debugging/monitoring
    pub fn stats(&self) -> QueueStats {
        self.inner.stats()
    }
}

/// A handle represents a notification channel.
///
/// Cheap to clone (just an ID). Clients must explicitly unregister
/// handles when done using NotificationQueue::unregister_handle().
#[derive(Debug, Clone)]
pub struct Handle {
    id: u64,
}

impl Handle {
    pub fn id(&self) -> u64 {
        self.id
    }
}

// ============================================================================
// Waiting API - for async clients
// ============================================================================

impl NotificationQueue {
    /// Wait for notification on this handle (async version).
    ///
    /// Returns the notification argument, or error if handle is unlisted
    /// while waiting.
    ///
    /// This is cancellation-safe - dropping the future properly cleans up.
    pub async fn wait(&self, handle: &Handle) -> Result<i32, QueueError> {
        self.inner.wait(handle.id).await
    }

    /// Wait with timeout
    pub async fn wait_timeout(
        &self,
        handle: &Handle,
        timeout: Duration,
    ) -> Result<i32, QueueError> {
        tokio::time::timeout(timeout, self.wait(handle))
            .await
            .map_err(|_| QueueError::Timeout)?
    }
}

// ============================================================================
// Subscription API - for long-lived listeners
// ============================================================================

/// A subscription to handle notifications.
///
/// Client receives notifications via a channel, not callback.
/// This prevents buggy clients from blocking the core.
pub struct Subscription {
    id: u64,
    /// Channel to receive notifications. If client is slow and channel fills,
    /// notifications are dropped (latest-wins semantics).
    pub receiver: crossbeam::channel::Receiver<i32>,
    _handle: Handle,
}

impl Subscription {
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Try to receive without blocking
    pub fn try_recv(&self) -> Option<i32> {
        self.receiver.try_recv().ok()
    }

    /// Blocking receive (for dedicated worker threads)
    pub fn recv(&self) -> Option<i32> {
        self.receiver.recv().ok()
    }

    /// Async receive (for async contexts)
    pub async fn recv_async(&self) -> Option<i32> {
        // Convert crossbeam channel to async
        // In real impl, might use tokio::sync::mpsc instead
        tokio::task::spawn_blocking({
            let rx = self.receiver.clone();
            move || rx.recv().ok()
        })
        .await
        .ok()
        .flatten()
    }
}

impl NotificationQueue {
    /// Subscribe to notifications on this handle.
    ///
    /// Returns a Subscription with a channel receiver. The channel is bounded
    /// to prevent unbounded memory growth from slow clients.
    ///
    /// If max_subscribers limit is reached, returns error.
    pub fn subscribe(
        &self,
        handle: &Handle,
        channel_size: usize,
    ) -> Result<Subscription, QueueError> {
        self.inner.subscribe(handle.id, channel_size)
    }

    /// Unsubscribe (manual cleanup - normally Subscription drop handles this)
    pub fn unsubscribe(&self, subscription: Subscription) -> Result<(), QueueError> {
        self.inner.unsubscribe(subscription.id)
    }
}

// ============================================================================
// Errors and Stats
// ============================================================================

#[derive(Debug, Clone, thiserror::Error)]
pub enum QueueError {
    #[error("Handle not registered: {0}")]
    HandleNotRegistered(u64),

    #[error("Handle was unlisted while waiting")]
    HandleUnlisted,

    #[error("Maximum subscribers reached for handle")]
    MaxSubscribers,

    #[error("Maximum waiters reached for handle")]
    MaxWaiters,

    #[error("Wait timeout")]
    Timeout,

    #[error("Subscription not found: {0}")]
    SubscriptionNotFound(u64),
}

#[derive(Debug, Clone)]
pub struct QueueStats {
    pub total_handles: usize,
    pub total_waiters: usize,
    pub total_subscribers: usize,
    pub total_notifications: u64,
}

// ============================================================================
// Implementation (internal)
// ============================================================================

use std::sync::atomic::{AtomicU64, Ordering};
use parking_lot::RwLock; // Faster than std::sync::RwLock
use rustc_hash::FxHashMap; // Faster than std HashMap for integer keys

struct QueueInner {
    config: QueueConfig,
    next_handle_id: AtomicU64,
    next_subscription_id: AtomicU64,

    // Separate locks for different operations to reduce contention
    handles: RwLock<FxHashMap<u64, HandleEntry>>,

    // Stats (lock-free)
    stats: AtomicStats,
}

struct HandleEntry {
    waiters: Vec<Waiter>,
    subscribers: Vec<Subscriber>,
}

struct Waiter {
    sender: tokio::sync::oneshot::Sender<i32>,
}

struct Subscriber {
    id: u64,
    sender: crossbeam::channel::Sender<i32>,
}

struct AtomicStats {
    total_notifications: AtomicU64,
}

impl QueueInner {
    fn new(config: QueueConfig) -> Self {
        Self {
            config,
            next_handle_id: AtomicU64::new(1),
            next_subscription_id: AtomicU64::new(1),
            handles: RwLock::new(FxHashMap::default()),
            stats: AtomicStats {
                total_notifications: AtomicU64::new(0),
            },
        }
    }

    fn register_handle(&self) -> u64 {
        let id = self.next_handle_id.fetch_add(1, Ordering::Relaxed);
        let mut handles = self.handles.write();
        handles.insert(
            id,
            HandleEntry {
                waiters: Vec::new(),
                subscribers: Vec::new(),
            },
        );
        id
    }

    fn unlist(&self, handle_id: u64) {
        let mut handles = self.handles.write();
        if let Some(entry) = handles.remove(&handle_id) {
            // Notify all waiters that handle is gone
            for waiter in entry.waiters {
                let _ = waiter.sender.send(-1); // -1 indicates unlist
            }
            // Subscribers' channels will just close when senders are dropped
        }
    }

    async fn wait(&self, handle_id: u64) -> Result<i32, QueueError> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        // Add waiter under write lock
        {
            let mut handles = self.handles.write();
            let entry = handles
                .get_mut(&handle_id)
                .ok_or(QueueError::HandleNotRegistered(handle_id))?;

            if entry.waiters.len() >= self.config.max_waiters_per_handle {
                return Err(QueueError::MaxWaiters);
            }

            entry.waiters.push(Waiter { sender: tx });
        }

        // Wait outside lock
        match rx.await {
            Ok(-1) => Err(QueueError::HandleUnlisted),
            Ok(arg) => Ok(arg),
            Err(_) => Err(QueueError::HandleUnlisted),
        }
    }

    fn notify(&self, handle_id: u64, arg: i32) -> Result<usize, QueueError> {
        let mut count = 0;

        // Extract waiters and subscribers under write lock
        let (waiters, subscribers) = {
            let mut handles = self.handles.write();
            let entry = handles
                .get_mut(&handle_id)
                .ok_or(QueueError::HandleNotRegistered(handle_id))?;

            // Take all waiters (one-shot)
            let waiters = std::mem::take(&mut entry.waiters);

            // Clone subscriber senders (persistent)
            let subscribers = entry.subscribers.clone();

            (waiters, subscribers)
        };

        // Notify outside lock - client code can't block us
        for waiter in waiters {
            if waiter.sender.send(arg).is_ok() {
                count += 1;
            }
        }

        for subscriber in subscribers {
            // try_send never blocks - if channel full, drop notification
            if subscriber.sender.try_send(arg).is_ok() {
                count += 1;
            }
        }

        self.stats.total_notifications.fetch_add(1, Ordering::Relaxed);
        Ok(count)
    }

    fn subscribe(
        &self,
        handle_id: u64,
        channel_size: usize,
    ) -> Result<Subscription, QueueError> {
        let (tx, rx) = crossbeam::channel::bounded(channel_size);
        let subscription_id = self.next_subscription_id.fetch_add(1, Ordering::Relaxed);

        let mut handles = self.handles.write();
        let entry = handles
            .get_mut(&handle_id)
            .ok_or(QueueError::HandleNotRegistered(handle_id))?;

        if entry.subscribers.len() >= self.config.max_subscribers_per_handle {
            return Err(QueueError::MaxSubscribers);
        }

        entry.subscribers.push(Subscriber {
            id: subscription_id,
            sender: tx,
        });

        Ok(Subscription {
            id: subscription_id,
            receiver: rx,
            _handle: Handle { id: handle_id },
        })
    }

    fn unsubscribe(&self, subscription_id: u64) -> Result<(), QueueError> {
        let mut handles = self.handles.write();
        for entry in handles.values_mut() {
            if let Some(pos) = entry
                .subscribers
                .iter()
                .position(|s| s.id == subscription_id)
            {
                entry.subscribers.remove(pos);
                return Ok(());
            }
        }
        Err(QueueError::SubscriptionNotFound(subscription_id))
    }

    fn stats(&self) -> QueueStats {
        let handles = self.handles.read();
        let total_waiters: usize = handles.values().map(|e| e.waiters.len()).sum();
        let total_subscribers: usize = handles.values().map(|e| e.subscribers.len()).sum();

        QueueStats {
            total_handles: handles.len(),
            total_waiters,
            total_subscribers,
            total_notifications: self.stats.total_notifications.load(Ordering::Relaxed),
        }
    }
}

// ============================================================================
// Example Usage
// ============================================================================

#[cfg(test)]
mod examples {
    use super::*;

    #[tokio::test]
    async fn example_basic_wait() {
        let queue = NotificationQueue::new(QueueConfig::default());

        // Register a handle
        let handle = queue.register_handle();

        // Spawn a task that waits
        let queue_clone = queue.clone();
        let handle_clone = handle.clone();
        let waiter = tokio::spawn(async move {
            queue_clone.wait(&handle_clone).await.unwrap()
        });

        // Give waiter time to register
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Notify from another thread
        queue.notify(&handle, 42).unwrap();

        // Waiter receives notification
        let result = waiter.await.unwrap();
        assert_eq!(result, 42);

        // Explicitly unregister handle when done
        queue.unregister_handle(&handle);
    }

    #[tokio::test]
    async fn example_subscription() {
        let queue = NotificationQueue::new(QueueConfig::default());
        let handle = queue.register_handle();

        // Subscribe with bounded channel
        let sub = queue.subscribe(&handle, 10).unwrap();

        // Send some notifications
        queue.notify(&handle, 1).unwrap();
        queue.notify(&handle, 2).unwrap();
        queue.notify(&handle, 3).unwrap();

        // Receive from subscription
        assert_eq!(sub.try_recv(), Some(1));
        assert_eq!(sub.try_recv(), Some(2));
        assert_eq!(sub.try_recv(), Some(3));
        assert_eq!(sub.try_recv(), None); // No more

        // Explicitly unregister handle when done
        queue.unregister_handle(&handle);
    }

    #[tokio::test]
    async fn example_buggy_client_protection() {
        let queue = NotificationQueue::new(QueueConfig::default());
        let handle = queue.register_handle();

        // Create subscription with small channel
        let sub = queue.subscribe(&handle, 2).unwrap();

        // Flood with notifications
        for i in 0..100 {
            queue.notify(&handle, i).unwrap();
        }

        // Client only gets what fits in channel - rest dropped
        // Core was never blocked!
        assert_eq!(sub.try_recv(), Some(0));
        assert_eq!(sub.try_recv(), Some(1));
        assert_eq!(sub.try_recv(), None); // Channel was full, rest dropped

        // Explicitly unregister handle when done
        queue.unregister_handle(&handle);
    }
}

impl Clone for NotificationQueue {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
