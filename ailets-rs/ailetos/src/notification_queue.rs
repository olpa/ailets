//! Notification Queue - Working Implementation
//!
//! Thread-safe notification mechanism for coordinating between sync workers
//! and async clients. Designed for OS core use with protection against buggy clients.

use parking_lot::RwLock;
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

// ============================================================================
// Configuration
// ============================================================================

#[derive(Debug, Clone)]
pub struct QueueConfig {
    pub max_subscribers_per_handle: usize,
    pub max_waiters_per_handle: usize,
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

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug, Clone, Error)]
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

// ============================================================================
// Public API Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct Handle {
    id: u64,
    debug_hint: String,
}

impl Handle {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn debug_hint(&self) -> &str {
        &self.debug_hint
    }
}

pub struct HandleGuard {
    inner: Arc<QueueInner>,
    handle: Handle,
}

impl HandleGuard {
    fn new(inner: Arc<QueueInner>, debug_hint: String) -> Self {
        let id = inner.register_handle(&debug_hint);
        Self {
            inner,
            handle: Handle { id, debug_hint },
        }
    }

    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    pub fn into_handle(self) -> Handle {
        let handle = self.handle.clone();
        std::mem::forget(self);
        handle
    }
}

impl Drop for HandleGuard {
    fn drop(&mut self) {
        self.inner.unlist(self.handle.id);
    }
}

#[derive(Debug, Clone)]
pub struct QueueStats {
    pub total_handles: usize,
    pub total_waiters: usize,
    pub total_subscribers: usize,
    pub total_notifications: u64,
}

pub struct Subscription {
    id: u64,
    pub receiver: crossbeam::channel::Receiver<i32>,
    _handle: Handle,
}

impl Subscription {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn try_recv(&self) -> Option<i32> {
        self.receiver.try_recv().ok()
    }

    pub fn recv(&self) -> Option<i32> {
        self.receiver.recv().ok()
    }
}

// ============================================================================
// Main Queue
// ============================================================================

#[derive(Clone)]
pub struct NotificationQueue {
    inner: Arc<QueueInner>,
}

impl NotificationQueue {
    pub fn new(config: QueueConfig) -> Self {
        Self {
            inner: Arc::new(QueueInner::new(config)),
        }
    }

    pub fn register_handle(&self, debug_hint: impl Into<String>) -> HandleGuard {
        HandleGuard::new(self.inner.clone(), debug_hint.into())
    }

    pub fn notify(&self, handle: &Handle, arg: i32) -> Result<usize, QueueError> {
        self.inner.notify(handle.id, arg)
    }

    pub async fn wait(&self, handle: &Handle) -> Result<i32, QueueError> {
        self.inner.wait(handle.id).await
    }

    /// Get the internal lock for atomic condition-check + register operations.
    ///
    /// Use this to atomically check a condition and register a waiter.
    pub fn get_lock(&self) -> parking_lot::RwLockWriteGuard<'_, FxHashMap<u64, HandleEntry>> {
        self.inner.handles.write()
    }

    /// Register a waiter under lock and return receiver for waiting.
    ///
    /// This is the synchronous part of the wait operation. Call this while
    /// holding a lock to atomically check a condition and register a waiter.
    ///
    /// Returns a receiver that will be notified when the handle is triggered.
    pub fn register_waiter_locked(
        &self,
        handle: &Handle,
        mut lock: parking_lot::RwLockWriteGuard<'_, FxHashMap<u64, HandleEntry>>,
    ) -> Result<tokio::sync::oneshot::Receiver<i32>, QueueError> {
        let (tx, rx) = tokio::sync::oneshot::channel();

        let entry = lock
            .get_mut(&handle.id)
            .ok_or(QueueError::HandleNotRegistered(handle.id))?;

        if entry.waiters.len() >= self.inner.config.max_waiters_per_handle {
            return Err(QueueError::MaxWaiters);
        }

        entry.waiters.push(Waiter {
            sender: tx,
            debug_hint: "wait_unsafe".to_string(),
        });

        // Lock is dropped here
        drop(lock);

        Ok(rx)
    }

    /// Wait on a receiver obtained from register_waiter_locked.
    pub async fn wait_on_receiver(
        &self,
        rx: tokio::sync::oneshot::Receiver<i32>,
    ) -> Result<i32, QueueError> {
        match rx.await {
            Ok(-1) => Err(QueueError::HandleUnlisted),
            Ok(arg) => Ok(arg),
            Err(_) => Err(QueueError::HandleUnlisted),
        }
    }

    pub async fn wait_timeout(
        &self,
        handle: &Handle,
        timeout: Duration,
    ) -> Result<i32, QueueError> {
        tokio::time::timeout(timeout, self.wait(handle))
            .await
            .map_err(|_| QueueError::Timeout)?
    }

    pub fn subscribe(
        &self,
        handle: &Handle,
        channel_size: usize,
        debug_hint: impl Into<String>,
    ) -> Result<Subscription, QueueError> {
        self.inner.subscribe(handle.id, channel_size, debug_hint.into())
    }

    pub fn unsubscribe(&self, subscription: Subscription) -> Result<(), QueueError> {
        self.inner.unsubscribe(subscription.id)
    }

    pub fn stats(&self) -> QueueStats {
        self.inner.stats()
    }
}

// ============================================================================
// Internal Implementation
// ============================================================================

struct QueueInner {
    config: QueueConfig,
    next_handle_id: AtomicU64,
    next_subscription_id: AtomicU64,
    handles: RwLock<FxHashMap<u64, HandleEntry>>,
    stats: AtomicStats,
}

pub(crate) struct HandleEntry {
    debug_hint: String,
    waiters: Vec<Waiter>,
    subscribers: Vec<Subscriber>,
}

struct Waiter {
    sender: tokio::sync::oneshot::Sender<i32>,
    debug_hint: String,
}

#[derive(Clone)]
struct Subscriber {
    id: u64,
    sender: crossbeam::channel::Sender<i32>,
    debug_hint: String,
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

    fn register_handle(&self, debug_hint: &str) -> u64 {
        let id = self.next_handle_id.fetch_add(1, Ordering::Relaxed);
        let mut handles = self.handles.write();
        handles.insert(
            id,
            HandleEntry {
                debug_hint: debug_hint.to_string(),
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
                let _ = waiter.sender.send(-1);
            }
            // Subscribers' channels will close when senders are dropped
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

            entry.waiters.push(Waiter {
                sender: tx,
                debug_hint: "async_wait".to_string(),
            });
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

        // Extract waiters and clone subscribers under write lock
        let (waiters, subscribers) = {
            let mut handles = self.handles.write();
            let entry = handles
                .get_mut(&handle_id)
                .ok_or(QueueError::HandleNotRegistered(handle_id))?;

            let waiters = std::mem::take(&mut entry.waiters);
            let subscribers = entry.subscribers.clone();

            (waiters, subscribers)
        };

        // Notify outside lock
        for waiter in waiters {
            if waiter.sender.send(arg).is_ok() {
                count += 1;
            }
        }

        for subscriber in subscribers {
            if subscriber.sender.try_send(arg).is_ok() {
                count += 1;
            }
        }

        self.stats
            .total_notifications
            .fetch_add(1, Ordering::Relaxed);
        Ok(count)
    }

    fn subscribe(
        &self,
        handle_id: u64,
        channel_size: usize,
        debug_hint: String,
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
            debug_hint: debug_hint.clone(),
        });

        Ok(Subscription {
            id: subscription_id,
            receiver: rx,
            _handle: Handle {
                id: handle_id,
                debug_hint,
            },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_wait_notify() {
        let queue = NotificationQueue::new(QueueConfig::default());
        let guard = queue.register_handle("test");
        let handle = guard.handle().clone();

        let queue_clone = queue.clone();
        let handle_clone = handle.clone();
        let waiter = tokio::spawn(async move { queue_clone.wait(&handle_clone).await.unwrap() });

        tokio::time::sleep(Duration::from_millis(10)).await;
        queue.notify(&handle, 42).unwrap();

        let result = waiter.await.unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_subscription() {
        let queue = NotificationQueue::new(QueueConfig::default());
        let guard = queue.register_handle("test");
        let handle = guard.handle().clone();

        let sub = queue.subscribe(&handle, 10, "subscriber").unwrap();

        queue.notify(&handle, 1).unwrap();
        queue.notify(&handle, 2).unwrap();

        assert_eq!(sub.try_recv(), Some(1));
        assert_eq!(sub.try_recv(), Some(2));
        assert_eq!(sub.try_recv(), None);
    }
}
