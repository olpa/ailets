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
