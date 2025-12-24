use crate::notification_queue::*;

#[tokio::test]
async fn test_basic_wait_notify() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(1);

    queue.whitelist(handle, "test");

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let queue_clone = queue.clone();
    let waiter = tokio::spawn(async move {
        let lock = queue_clone.get_lock();
        let wait_future = queue_clone.wait_async(handle, "waiter", lock);
        // Signal that we've registered the waiter
        ready_tx.send(()).unwrap();
        wait_future.await;
    });

    // Wait for the waiter to be registered
    ready_rx.await.unwrap();
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

    // Receive notifications (subscribers receive the arg value)
    assert_eq!(rx.recv().await.unwrap(), 1);
    assert_eq!(rx.recv().await.unwrap(), 2);

    // Drop receiver to unsubscribe (automatic)
    drop(rx);

    queue.unlist(handle);
}

#[tokio::test]
async fn test_unlist_notifies_waiters() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(3);

    queue.whitelist(handle, "test");

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let queue_clone = queue.clone();
    let waiter = tokio::spawn(async move {
        let lock = queue_clone.get_lock();
        let wait_future = queue_clone.wait_async(handle, "waiter", lock);
        // Signal that we've registered the waiter
        ready_tx.send(()).unwrap();
        wait_future.await;
    });

    // Wait for the waiter to be registered
    ready_rx.await.unwrap();
    queue.unlist(handle);

    waiter.await.unwrap();
}

// ============================================================================
// Test 1: Race Condition Handling
// ============================================================================

#[tokio::test]
async fn test_race_condition_notify_before_wait() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(10);

    // 1. Whitelist handle
    queue.whitelist(handle, "test");

    // 2. Notify (no waiters yet - simulates worker finishing before client waits)
    queue.notify(handle, 42);

    // 3. Unlist handle (simulates handle being freed)
    queue.unlist(handle);

    // 4. Try to wait_async - should exit immediately (handle not whitelisted)
    let lock = queue.get_lock();
    queue.wait_async(handle, "waiter", lock).await;
    // If we reach here without hanging, the test passes
}

// ============================================================================
// Test 2: Multiple Waiters on Same Handle
// ============================================================================

#[tokio::test]
async fn test_multiple_waiters() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(11);

    queue.whitelist(handle, "test");

    // Create 3 waiters
    let (ready_tx1, ready_rx1) = tokio::sync::oneshot::channel();
    let (ready_tx2, ready_rx2) = tokio::sync::oneshot::channel();
    let (ready_tx3, ready_rx3) = tokio::sync::oneshot::channel();

    let queue_clone1 = queue.clone();
    let waiter1 = tokio::spawn(async move {
        let lock = queue_clone1.get_lock();
        let wait_future = queue_clone1.wait_async(handle, "waiter1", lock);
        ready_tx1.send(()).unwrap();
        wait_future.await;
    });

    let queue_clone2 = queue.clone();
    let waiter2 = tokio::spawn(async move {
        let lock = queue_clone2.get_lock();
        let wait_future = queue_clone2.wait_async(handle, "waiter2", lock);
        ready_tx2.send(()).unwrap();
        wait_future.await;
    });

    let queue_clone3 = queue.clone();
    let waiter3 = tokio::spawn(async move {
        let lock = queue_clone3.get_lock();
        let wait_future = queue_clone3.wait_async(handle, "waiter3", lock);
        ready_tx3.send(()).unwrap();
        wait_future.await;
    });

    // Wait for all waiters to be registered
    ready_rx1.await.unwrap();
    ready_rx2.await.unwrap();
    ready_rx3.await.unwrap();

    // Notify once - all waiters should wake up
    queue.notify(handle, 100);

    // All waiters should complete
    waiter1.await.unwrap();
    waiter2.await.unwrap();
    waiter3.await.unwrap();

    queue.unlist(handle);
}

// ============================================================================
// Test 4: Mixed Waiters and Subscribers
// ============================================================================

#[tokio::test]
async fn test_mixed_waiters_and_subscribers() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(12);

    queue.whitelist(handle, "test");

    // Create a subscriber
    let mut rx = queue.subscribe(handle, 10, "subscriber").unwrap();

    // Create a waiter
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let queue_clone = queue.clone();
    let waiter = tokio::spawn(async move {
        let lock = queue_clone.get_lock();
        let wait_future = queue_clone.wait_async(handle, "waiter", lock);
        ready_tx.send(()).unwrap();
        wait_future.await;
    });

    // Wait for waiter to be registered
    ready_rx.await.unwrap();

    // Notify once - both should receive
    queue.notify(handle, 123);

    // Waiter should complete
    waiter.await.unwrap();

    // Subscriber should receive the arg value
    assert_eq!(rx.recv().await.unwrap(), 123);

    queue.unlist(handle);
}

// ============================================================================
// Test 5: Wait on Non-Whitelisted Handle
// ============================================================================

#[tokio::test]
async fn test_wait_on_non_whitelisted_handle() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(13);

    // Don't whitelist the handle

    // Try to wait - should exit immediately
    let lock = queue.get_lock();
    queue.wait_async(handle, "waiter", lock).await;
    // If we reach here immediately without hanging, the test passes
}

// ============================================================================
// Test 6: Subscribe to Non-Whitelisted Handle
// ============================================================================

#[tokio::test]
async fn test_subscribe_to_non_whitelisted_handle() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(14);

    // Don't whitelist the handle

    // Try to subscribe - should return None
    let result = queue.subscribe(handle, 10, "subscriber");
    assert!(result.is_none());
}

// ============================================================================
// Test 7: Notify with No Waiters/Subscribers
// ============================================================================

#[tokio::test]
async fn test_notify_with_no_waiters() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(15);

    queue.whitelist(handle, "test");

    // Notify without any waiters - should not panic
    queue.notify(handle, 42);

    queue.unlist(handle);
}

// ============================================================================
// Test 8: Unlist Removes Broadcast Channels
// ============================================================================

#[tokio::test]
async fn test_unlist_removes_broadcast_channels() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(16);

    queue.whitelist(handle, "test");

    // Subscribe to handle
    let mut rx = queue.subscribe(handle, 10, "subscriber").unwrap();

    // Unlist handle (delete_subscribed=true)
    queue.unlist(handle);

    // Subscriber should receive -1
    assert_eq!(rx.recv().await.unwrap(), -1);

    // Further receives should fail (channel closed)
    assert!(rx.recv().await.is_err());
}

// ============================================================================
// Test 9: Dropped Subscription Receiver
// ============================================================================

#[tokio::test]
async fn test_dropped_subscription_receiver() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(17);

    queue.whitelist(handle, "test");

    // Subscribe and immediately drop receiver
    let rx = queue.subscribe(handle, 10, "subscriber").unwrap();
    drop(rx);

    // Notify - should not panic even though receiver is dropped
    queue.notify(handle, 42);

    queue.unlist(handle);
}

// ============================================================================
// Test 11: Concurrent Operations
// ============================================================================

#[tokio::test]
async fn test_concurrent_operations() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(18);

    queue.whitelist(handle, "test");

    // Spawn multiple threads doing different operations
    let mut task_handles = vec![];

    // Multiple subscribers first
    for _ in 0..3 {
        let queue_clone = queue.clone();
        task_handles.push(tokio::spawn(async move {
            if let Some(mut rx) = queue_clone.subscribe(handle, 10, "concurrent_subscriber") {
                // Try to receive one notification
                let _ = rx.recv().await;
            }
        }));
    }

    // Multiple waiters with ready signals
    let mut ready_channels = vec![];
    for _ in 0..3 {
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        ready_channels.push(ready_rx);

        let queue_clone = queue.clone();
        task_handles.push(tokio::spawn(async move {
            let lock = queue_clone.get_lock();
            let wait_future = queue_clone.wait_async(handle, "concurrent_waiter", lock);
            let _ = ready_tx.send(());
            wait_future.await;
        }));
    }

    // Wait for all waiters to be ready
    for ready_rx in ready_channels {
        let _ = ready_rx.await;
    }

    // Now send notifications - multiple notifiers
    for i in 0..5 {
        let queue_clone = queue.clone();
        task_handles.push(tokio::spawn(async move {
            queue_clone.notify(handle, i);
        }));
    }

    // Wait for all operations to complete
    for task_handle in task_handles {
        task_handle.await.unwrap();
    }

    queue.unlist(handle);
}

// ============================================================================
// Test 13: Notification Argument Values
// ============================================================================

#[tokio::test]
async fn test_notification_argument_values() {
    let queue = NotificationQueueArc::new();
    let handle = Handle::new(19);

    queue.whitelist(handle, "test");

    let mut rx = queue.subscribe(handle, 10, "subscriber").unwrap();

    // Send different arg values
    let test_values = [0, 42, -1, 12345, -9999];
    for &val in &test_values {
        queue.notify(handle, val);
    }

    // Subscribers should receive exact values
    for &expected in &test_values {
        assert_eq!(rx.recv().await.unwrap(), expected);
    }

    queue.unlist(handle);
}
