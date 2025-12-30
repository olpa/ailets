use ailetos::mempipe::{MemPipe, MemPipeError};
use ailetos::notification_queue::{Handle, NotificationQueueArc};
use embedded_io_async::Read;

#[tokio::test]
async fn test_write_read() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = MemPipe::new(writer_handle, queue.clone(), "test", None);

    let mut reader = pipe.get_reader(Handle::new(2));
    let _reader_handle = *reader.handle();

    // Write some data
    pipe.writer().write_sync(b"Hello").unwrap();

    // Read it back
    let mut buf = [0u8; 10];
    let n = reader.read(&mut buf).await.unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf[..n], b"Hello");

    // Writer unregisters its handle on drop
}

#[tokio::test]
async fn test_multiple_readers() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = MemPipe::new(writer_handle, queue.clone(), "test", None);

    let mut reader1 = pipe.get_reader(Handle::new(2));
    let _reader1_handle = *reader1.handle();
    let mut reader2 = pipe.get_reader(Handle::new(3));
    let _reader2_handle = *reader2.handle();

    // Write data
    pipe.writer().write_sync(b"Broadcast").unwrap();

    // Both readers should get the same data
    let mut buf1 = [0u8; 20];
    let mut buf2 = [0u8; 20];

    let n1 = reader1.read(&mut buf1).await.unwrap();
    let n2 = reader2.read(&mut buf2).await.unwrap();

    assert_eq!(n1, 9);
    assert_eq!(n2, 9);
    assert_eq!(&buf1[..n1], b"Broadcast");
    assert_eq!(&buf2[..n2], b"Broadcast");

    // Writer unregisters its handle on drop
}

#[tokio::test]
async fn test_close_propagation() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = MemPipe::new(writer_handle, queue.clone(), "test", None);

    let mut reader = pipe.get_reader(Handle::new(2));
    let _reader_handle = *reader.handle();

    // Write and close
    pipe.writer().write_sync(b"Data").unwrap();
    pipe.writer().close().unwrap();

    // Reader should get data
    let mut buf = [0u8; 10];
    let n = reader.read(&mut buf).await.unwrap();
    assert_eq!(n, 4);

    // Second read should get EOF
    let n = reader.read(&mut buf).await.unwrap();
    assert_eq!(n, 0);

    // Writer unregisters its handle on drop
}

#[tokio::test]
async fn test_write_notifies_observers() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = MemPipe::new(writer_handle, queue.clone(), "test", None);

    // Subscribe to writer's handle to observe notifications directly
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Write non-empty data - this should notify observers
    let n = pipe.writer().write_sync(b"Hello").unwrap();
    assert_eq!(n, 5);

    // Verify notification was sent
    let notification = subscriber.recv().await.expect("Should receive notification");
    assert_eq!(notification, 5); // Should notify with the number of bytes written
}

#[tokio::test]
async fn test_empty_write_does_not_notify() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = MemPipe::new(writer_handle, queue.clone(), "test", None);

    // Subscribe to writer's handle to observe notifications directly
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Empty write should succeed and return 0
    let n = pipe.writer().write_sync(b"").unwrap();
    assert_eq!(n, 0);

    // Verify NO notification was sent for empty write
    // Use try_recv which doesn't block - should return Err(Empty)
    let result = subscriber.try_recv();
    assert!(result.is_err()); // Should be empty, no notification sent

    // Now write actual data
    let n = pipe.writer().write_sync(b"Hello").unwrap();
    assert_eq!(n, 5);

    // Verify notification WAS sent for non-empty write
    let notification = subscriber.recv().await.expect("Should receive notification");
    assert_eq!(notification, 5);

    // Another empty write after real data
    let n = pipe.writer().write_sync(b"").unwrap();
    assert_eq!(n, 0);

    // Again, verify NO notification for empty write
    let result = subscriber.try_recv();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_empty_write_on_closed_writer() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = MemPipe::new(writer_handle, queue.clone(), "test", None);

    // Close the writer
    pipe.writer().close().unwrap();

    // Empty write on closed writer should return error
    let result = pipe.writer().write_sync(b"");
    assert!(matches!(result, Err(MemPipeError::WriterClosed)));
}

#[tokio::test]
async fn test_empty_write_with_errno() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = MemPipe::new(writer_handle, queue.clone(), "test", None);

    // Set error
    pipe.writer().set_error(42).unwrap();

    // Empty write should return error, not Ok(0)
    let result = pipe.writer().write_sync(b"");
    assert!(matches!(result, Err(MemPipeError::WriterError(42))));
}

#[tokio::test]
async fn test_set_error_ignored_when_writer_closed() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = MemPipe::new(writer_handle, queue.clone(), "test", None);

    // Close the writer
    pipe.writer().close().unwrap();

    // Setting error on closed writer should be ignored (returns Ok)
    let result = pipe.writer().set_error(42);
    assert!(result.is_ok());

    // Error should still be 0 (not set)
    assert_eq!(pipe.writer().get_error(), 0);
}

#[tokio::test]
async fn test_set_error_does_not_notify_when_writer_closed() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = MemPipe::new(writer_handle, queue.clone(), "test", None);

    // Subscribe to writer's handle to observe notifications
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Close the writer - this will send a notification with -1
    pipe.writer().close().unwrap();

    // Receive the close notification
    let notification = subscriber.recv().await.expect("Should receive close notification");
    assert_eq!(notification, -1);

    // Now try to set error on the closed writer
    let result = pipe.writer().set_error(42);
    assert!(result.is_ok());

    // Verify NO additional notification was sent
    let result = subscriber.try_recv();
    assert!(result.is_err()); // Should be empty, no notification sent
}
