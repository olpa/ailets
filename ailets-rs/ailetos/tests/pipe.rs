use ailetos::pipe::{Pipe, Buffer};
use ailetos::notification_queue::{Handle, NotificationQueueArc};
use std::sync::{Arc, Mutex};

// Wrapper type for Vec<u8> to implement Buffer
struct VecBuffer(Vec<u8>);

impl VecBuffer {
    fn new() -> Self {
        Self(Vec::new())
    }
}

impl Buffer for VecBuffer {
    fn write(&mut self, data: &[u8]) -> isize {
        self.0.extend_from_slice(data);
        data.len() as isize
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

// Configurable buffer for testing different write() return values
struct ConfigurableBuffer {
    data: Vec<u8>,
    write_return: Arc<Mutex<Option<isize>>>,
}

impl ConfigurableBuffer {
    fn new(write_return: Arc<Mutex<Option<isize>>>) -> Self {
        Self {
            data: Vec::new(),
            write_return,
        }
    }
}

impl Buffer for ConfigurableBuffer {
    fn write(&mut self, data: &[u8]) -> isize {
        let return_value = self.write_return.lock().unwrap().take();

        if let Some(val) = return_value {
            if val > 0 {
                // Partial or full write
                let to_write = (val as usize).min(data.len());
                self.data.extend_from_slice(&data[..to_write]);
                val
            } else {
                // Error (0 or negative)
                val
            }
        } else {
            // Default: successful write
            self.data.extend_from_slice(data);
            data.len() as isize
        }
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn as_slice(&self) -> &[u8] {
        &self.data
    }
}

#[tokio::test]
async fn test_write_read() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    let mut reader = pipe.get_reader(Handle::new(2));
    let _reader_handle = *reader.handle();

    // Write some data
    let n = pipe.writer().write(b"Hello");
    assert_eq!(n, 5);

    // Read it back
    let mut buf = [0u8; 10];
    let n = reader.read(&mut buf).await;
    assert_eq!(n, 5);
    assert_eq!(&buf[..n as usize], b"Hello");

    // Writer unregisters its handle on drop
}

#[tokio::test]
async fn test_multiple_write_read_cycles() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    let mut reader = pipe.get_reader(Handle::new(2));

    // Cycle 1: write-write-read
    assert_eq!(pipe.writer().write(b"Hello"), 5);
    assert_eq!(pipe.writer().write(b" "), 1);
    assert_eq!(pipe.writer().write(b"World"), 5);

    let mut buf = [0u8; 20];
    let n = reader.read(&mut buf).await;
    assert_eq!(n, 11);
    assert_eq!(&buf[..n as usize], b"Hello World");

    // Cycle 2: write-write-read
    assert_eq!(pipe.writer().write(b"Foo"), 3);
    assert_eq!(pipe.writer().write(b"Bar"), 3);

    let n = reader.read(&mut buf).await;
    assert_eq!(n, 6);
    assert_eq!(&buf[..n as usize], b"FooBar");

    // Cycle 3: write-write-read
    assert_eq!(pipe.writer().write(b"Test"), 4);
    assert_eq!(pipe.writer().write(b"123"), 3);

    let n = reader.read(&mut buf).await;
    assert_eq!(n, 7);
    assert_eq!(&buf[..n as usize], b"Test123");

    // Cycle 4: single write, partial read
    assert_eq!(pipe.writer().write(b"LongMessage"), 11);

    let mut small_buf = [0u8; 5];
    let n = reader.read(&mut small_buf).await;
    assert_eq!(n, 5);
    assert_eq!(&small_buf[..], b"LongM");

    // Read remainder
    let n = reader.read(&mut buf).await;
    assert_eq!(n, 6);
    assert_eq!(&buf[..n as usize], b"essage");
}

#[tokio::test]
async fn test_multiple_readers() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    let mut reader1 = pipe.get_reader(Handle::new(2));
    let _reader1_handle = *reader1.handle();
    let mut reader2 = pipe.get_reader(Handle::new(3));
    let _reader2_handle = *reader2.handle();

    // Write data
    let n = pipe.writer().write(b"Broadcast");
    assert_eq!(n, 9);

    // Both readers should get the same data
    let mut buf1 = [0u8; 20];
    let mut buf2 = [0u8; 20];

    let n1 = reader1.read(&mut buf1).await;
    let n2 = reader2.read(&mut buf2).await;

    assert_eq!(n1, 9);
    assert_eq!(n2, 9);
    assert_eq!(&buf1[..n1 as usize], b"Broadcast");
    assert_eq!(&buf2[..n2 as usize], b"Broadcast");

    // Writer unregisters its handle on drop
}

#[tokio::test]
async fn test_close_sends_notification() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    // Subscribe to writer's handle to observe notifications
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Close the writer
    pipe.writer().close();

    // Verify notification was sent with -1
    let notification = subscriber.recv().await.expect("Should receive notification");
    assert_eq!(notification, -1);
}

#[tokio::test]
async fn test_close_unlists_handle() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    // Close the writer
    pipe.writer().close();

    // Try to subscribe to the handle - should return None because it's unlisted
    let result = queue.subscribe(writer_handle, 10, "test_subscriber");
    assert!(result.is_none()); // Should return None for unlisted handle
}

#[tokio::test]
async fn test_write_notifies_observers() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    // Subscribe to writer's handle to observe notifications directly
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Write non-empty data - this should notify observers
    let n = pipe.writer().write(b"Hello");
    assert_eq!(n, 5);

    // Verify notification was sent
    let notification = subscriber.recv().await.expect("Should receive notification");
    assert_eq!(notification, 5); // Should notify with the number of bytes written
}

#[tokio::test]
async fn test_empty_write_does_not_notify() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    // Subscribe to writer's handle to observe notifications directly
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Empty write should succeed and return 0
    let n = pipe.writer().write(b"");
    assert_eq!(n, 0);

    // Verify NO notification was sent for empty write
    // Use try_recv which doesn't block - should return Err(Empty)
    let result = subscriber.try_recv();
    assert!(result.is_err()); // Should be empty, no notification sent

    // Now write actual data
    let n = pipe.writer().write(b"Hello");
    assert_eq!(n, 5);

    // Verify notification WAS sent for non-empty write
    let notification = subscriber.recv().await.expect("Should receive notification");
    assert_eq!(notification, 5);

    // Another empty write after real data
    let n = pipe.writer().write(b"");
    assert_eq!(n, 0);

    // Again, verify NO notification for empty write
    let result = subscriber.try_recv();
    assert!(result.is_err());
}

#[tokio::test]
async fn test_empty_write_on_closed_writer() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    // Close the writer
    pipe.writer().close();

    // Empty write on closed writer should return -1 (error)
    let result = pipe.writer().write(b"");
    assert_eq!(result, -1);
}

#[tokio::test]
async fn test_empty_write_with_errno() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    // Set error
    pipe.writer().set_error(42);

    // Empty write should return -1 (error), not 0
    let result = pipe.writer().write(b"");
    assert_eq!(result, -1);
    assert_eq!(pipe.writer().get_error(), 42);
}

#[tokio::test]
async fn test_reader_dont_read_when_error() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    let mut reader = pipe.get_reader(Handle::new(2));

    // Write some data
    assert_eq!(pipe.writer().write(b"hello"), 5);

    // Set reader's own error
    reader.set_error(42);

    // Try to read - should return -1 (error) without reading data
    let mut buf = [0u8; 10];
    let result = reader.read(&mut buf).await;
    assert_eq!(result, -1);
    assert_eq!(reader.get_error(), 42);

    // Verify data was not read by clearing error and reading
    reader.set_error(0);
    let result = reader.read(&mut buf).await;
    assert_eq!(result, 5);
    assert_eq!(&buf[..5], b"hello");
}

#[tokio::test]
async fn test_reader_get_writer_error() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    let reader = pipe.get_reader(Handle::new(2));

    // Writer sets error
    pipe.writer().set_error(99);

    // Reader should see writer's error
    assert_eq!(reader.get_error(), 99);
}

#[tokio::test]
async fn test_reader_read_with_writer_error() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    let mut reader = pipe.get_reader(Handle::new(2));

    // Write some data
    assert_eq!(pipe.writer().write(b"test"), 4);

    // Reader reads the data successfully
    let mut buf = [0u8; 10];
    let result = reader.read(&mut buf).await;
    assert_eq!(result, 4);
    assert_eq!(&buf[..4], b"test");

    // Writer sets error
    pipe.writer().set_error(88);

    // Next read should return -1 (error)
    let result = reader.read(&mut buf).await;
    assert_eq!(result, -1);
    assert_eq!(reader.get_error(), 88);
}

#[tokio::test]
async fn test_reader_drains_buffer_before_error() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    // Write some data
    assert_eq!(pipe.writer().write(b"buffered"), 8);

    // Writer sets error while data is still unread
    pipe.writer().set_error(77);

    // Create reader after error is set
    let mut reader = pipe.get_reader(Handle::new(2));

    // Reader should still be able to read the buffered data
    let mut buf = [0u8; 10];
    let result = reader.read(&mut buf).await;
    assert_eq!(result, 8);
    assert_eq!(&buf[..8], b"buffered");

    // Now that buffer is drained, next read should return error
    let result = reader.read(&mut buf).await;
    assert_eq!(result, -1);
    assert_eq!(reader.get_error(), 77);
}

#[tokio::test]
async fn test_writer_error_notifies_reader() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    let mut reader = pipe.get_reader(Handle::new(2));

    // Subscribe to writer's handle to observe when notification is sent
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Spawn reader task that will wait
    let reader_task = tokio::spawn(async move {
        let mut buf = [0u8; 10];
        reader.read(&mut buf).await
    });

    // Writer sets error - should notify
    pipe.writer().set_error(55);

    // Verify notification was sent (negative errno)
    let notification = subscriber.recv().await.expect("Should receive notification");
    assert_eq!(notification, -55);

    // Reader should wake up with error (-1)
    let result = reader_task.await.unwrap();
    assert_eq!(result, -1);
}

#[tokio::test]
async fn test_reader_own_error_takes_precedence() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    let mut reader = pipe.get_reader(Handle::new(2));

    // Writer sets error
    pipe.writer().set_error(5);

    // Reader sets own error
    reader.set_error(10);

    // get_error() should return reader's own error
    assert_eq!(reader.get_error(), 10);

    // read() should return -1 with reader's error
    let mut buf = [0u8; 10];
    let result = reader.read(&mut buf).await;
    assert_eq!(result, -1);
    assert_eq!(reader.get_error(), 10);
}

#[tokio::test]
async fn test_reader_error_checked_before_writer() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    let mut reader = pipe.get_reader(Handle::new(2));

    // Write some data
    assert_eq!(pipe.writer().write(b"data"), 4);

    // Reader sets own error first
    reader.set_error(15);

    // Writer sets error after
    pipe.writer().set_error(20);

    // Reader should see its own error
    assert_eq!(reader.get_error(), 15);

    // read() should return -1 with reader's error
    let mut buf = [0u8; 10];
    let result = reader.read(&mut buf).await;
    assert_eq!(result, -1);
}

#[tokio::test]
async fn test_multiple_readers_independent_errors() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    let mut reader1 = pipe.get_reader(Handle::new(2));
    let mut reader2 = pipe.get_reader(Handle::new(3));

    // Each reader sets different error
    reader1.set_error(100);
    reader2.set_error(200);

    // Each reader sees its own error
    assert_eq!(reader1.get_error(), 100);
    assert_eq!(reader2.get_error(), 200);

    // Both return -1 on read with their own errors
    let mut buf = [0u8; 10];
    assert_eq!(reader1.read(&mut buf).await, -1);
    assert_eq!(reader2.read(&mut buf).await, -1);
}

#[tokio::test]
async fn test_writer_set_error_notifies() {
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", VecBuffer::new());

    // Subscribe to writer's handle to observe notifications
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Writer sets error and notifies
    pipe.writer().set_error(123);

    // Verify notification was sent (negative errno)
    let notification = subscriber.recv().await.expect("Should receive notification");
    assert_eq!(notification, -123);
}

#[tokio::test]
async fn test_buffer_write_returns_zero() {
    // Test when buffer.write() returns 0 (buffer full)
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let write_return = Arc::new(Mutex::new(Some(0)));
    let buffer = ConfigurableBuffer::new(write_return.clone());
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", buffer);

    // Subscribe to observe notifications
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Write should return -1 when buffer returns 0
    let result = pipe.writer().write(b"test");
    assert_eq!(result, -1);

    // errno should be set to ENOSPC (28)
    assert_eq!(pipe.writer().get_error(), 28);

    // Notification should be -28 (negative ENOSPC)
    let notification = subscriber.recv().await.expect("Should receive notification");
    assert_eq!(notification, -28);
}

#[tokio::test]
async fn test_buffer_write_returns_negative_errno() {
    // Test when buffer.write() returns a negative errno
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    // Simulate buffer returning -5 (EIO - Input/output error)
    let write_return = Arc::new(Mutex::new(Some(-5)));
    let buffer = ConfigurableBuffer::new(write_return.clone());
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", buffer);

    // Subscribe to observe notifications
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Write should return -1 when buffer returns negative errno
    let result = pipe.writer().write(b"test");
    assert_eq!(result, -1);

    // errno should be set to 5 (the positive value of the error)
    assert_eq!(pipe.writer().get_error(), 5);

    // Notification should be -5 (the original negative errno from buffer)
    let notification = subscriber.recv().await.expect("Should receive notification");
    assert_eq!(notification, -5);
}

#[tokio::test]
async fn test_buffer_write_partial() {
    // Test when buffer.write() returns a value less than the data length (partial write)
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    // Simulate buffer accepting only 3 bytes out of 5
    let write_return = Arc::new(Mutex::new(Some(3)));
    let buffer = ConfigurableBuffer::new(write_return.clone());
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", buffer);

    // Subscribe to observe notifications
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Write should return 3 (partial write)
    let result = pipe.writer().write(b"hello");
    assert_eq!(result, 3);

    // No error should be set
    assert_eq!(pipe.writer().get_error(), 0);

    // Notification should be 3 (positive count of bytes written)
    let notification = subscriber.recv().await.expect("Should receive notification");
    assert_eq!(notification, 3);

    // Verify only 3 bytes were written to buffer
    assert_eq!(pipe.writer().tell(), 3);
}

#[tokio::test]
async fn test_buffer_write_full_success() {
    // Test when buffer.write() returns the full data length (successful write)
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let write_return = Arc::new(Mutex::new(Some(5)));
    let buffer = ConfigurableBuffer::new(write_return.clone());
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", buffer);

    // Subscribe to observe notifications
    let mut subscriber = queue
        .subscribe(writer_handle, 10, "test_subscriber")
        .expect("Failed to subscribe");

    // Write should return 5 (full write)
    let result = pipe.writer().write(b"hello");
    assert_eq!(result, 5);

    // No error should be set
    assert_eq!(pipe.writer().get_error(), 0);

    // Notification should be 5
    let notification = subscriber.recv().await.expect("Should receive notification");
    assert_eq!(notification, 5);

    // Verify all 5 bytes were written to buffer
    assert_eq!(pipe.writer().tell(), 5);
}

#[tokio::test]
async fn test_buffer_write_zero_after_successful_write() {
    // Test that a buffer returning 0 after a successful write properly sets error
    let queue = NotificationQueueArc::new();
    let writer_handle = Handle::new(1);

    let write_return = Arc::new(Mutex::new(None)); // First write succeeds
    let buffer = ConfigurableBuffer::new(write_return.clone());
    let pipe = Pipe::new(writer_handle, queue.clone(), "test", buffer);

    // First write succeeds
    let result = pipe.writer().write(b"test");
    assert_eq!(result, 4);
    assert_eq!(pipe.writer().get_error(), 0);

    // Second write returns 0 (buffer full)
    *write_return.lock().unwrap() = Some(0);
    let result = pipe.writer().write(b"more");
    assert_eq!(result, -1);
    assert_eq!(pipe.writer().get_error(), 28); // ENOSPC
}
