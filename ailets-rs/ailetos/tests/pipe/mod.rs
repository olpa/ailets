use ailetos::idgen::Handle;
use ailetos::pipe::{Reader, Writer};
use ailetos::{Buffer, EBADF, EOWNERDEAD, EPIPE};

#[tokio::test]
async fn test_write_read() {
    let writer_handle = Handle::new(1);

    let writer = Writer::new(writer_handle, "test", Buffer::new());
    let shared_data = writer.share_with_reader();
    let mut reader = Reader::new(Handle::new(2), shared_data);
    let _reader_handle = *reader.handle();

    // Write some data
    let n = writer.write(b"Hello");
    assert_eq!(n, Ok(5));

    // Read it back
    let mut buf = [0u8; 10];
    let n = reader.read(&mut buf).await;
    assert_eq!(n, Ok(5));
    assert_eq!(&buf[..n.unwrap()], b"Hello");
}

#[tokio::test]
async fn test_multiple_write_read_cycles() {
    let writer_handle = Handle::new(1);
    let writer = Writer::new(writer_handle, "test", Buffer::new());

    let shared_data = writer.share_with_reader();
    let mut reader = Reader::new(Handle::new(2), shared_data);

    // Cycle 1: write-write-read
    assert_eq!(writer.write(b"Hello"), Ok(5));
    assert_eq!(writer.write(b" "), Ok(1));
    assert_eq!(writer.write(b"World"), Ok(5));

    let mut buf = [0u8; 20];
    let n = reader.read(&mut buf).await;
    assert_eq!(n, Ok(11));
    assert_eq!(&buf[..n.unwrap()], b"Hello World");

    // Cycle 2: write-write-read
    assert_eq!(writer.write(b"Foo"), Ok(3));
    assert_eq!(writer.write(b"Bar"), Ok(3));

    let n = reader.read(&mut buf).await;
    assert_eq!(n, Ok(6));
    assert_eq!(&buf[..n.unwrap()], b"FooBar");

    // Cycle 3: write-write-read
    assert_eq!(writer.write(b"Test"), Ok(4));
    assert_eq!(writer.write(b"123"), Ok(3));

    let n = reader.read(&mut buf).await;
    assert_eq!(n, Ok(7));
    assert_eq!(&buf[..n.unwrap()], b"Test123");

    // Cycle 4: single write, partial read
    assert_eq!(writer.write(b"LongMessage"), Ok(11));

    let mut small_buf = [0u8; 5];
    let n = reader.read(&mut small_buf).await;
    assert_eq!(n, Ok(5));
    assert_eq!(&small_buf[..], b"LongM");

    // Read remainder
    let n = reader.read(&mut buf).await;
    assert_eq!(n, Ok(6));
    assert_eq!(&buf[..n.unwrap()], b"essage");
}

#[tokio::test]
async fn test_multiple_readers() {
    let writer_handle = Handle::new(1);

    let writer = Writer::new(writer_handle, "test", Buffer::new());

    let shared_data1 = writer.share_with_reader();
    let mut reader1 = Reader::new(Handle::new(2), shared_data1);
    let _reader1_handle = *reader1.handle();
    let shared_data2 = writer.share_with_reader();
    let mut reader2 = Reader::new(Handle::new(3), shared_data2);
    let _reader2_handle = *reader2.handle();

    // Write data
    let n = writer.write(b"Broadcast");
    assert_eq!(n, Ok(9));

    // Both readers should get the same data
    let mut buf1 = [0u8; 20];
    let mut buf2 = [0u8; 20];

    let n1 = reader1.read(&mut buf1).await;
    let n2 = reader2.read(&mut buf2).await;

    assert_eq!(n1, Ok(9));
    assert_eq!(n2, Ok(9));
    assert_eq!(&buf1[..n1.unwrap()], b"Broadcast");
    assert_eq!(&buf2[..n2.unwrap()], b"Broadcast");
}

#[tokio::test]
async fn test_empty_write_does_not_wake_waiting_reader() {
    let writer_handle = Handle::new(1);
    let writer = Writer::new(writer_handle, "test", Buffer::new());

    let shared_data = writer.share_with_reader();
    let mut reader = Reader::new(Handle::new(2), shared_data);

    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    // Spawn reader task that will block waiting for data
    tokio::spawn(async move {
        let mut buf = [0u8; 10];
        let n = reader.read(&mut buf).await;
        tx.send((n, buf)).await.unwrap();
    });

    // Empty write should NOT wake the reader
    let n = writer.write(b"");
    assert_eq!(n, Ok(0));

    // Verify reader has not received anything (still waiting)
    let result = rx.try_recv();
    assert!(
        result.is_err(),
        "Reader should still be waiting after empty write"
    );

    // Now write actual data - this SHOULD wake the reader
    let n = writer.write(b"Hello");
    assert_eq!(n, Ok(5));

    // Reader should wake up and send data
    let (n, buf) = rx.recv().await.expect("Should receive data from reader");
    assert_eq!(n, Ok(5));
    assert_eq!(&buf[..5], b"Hello");
}

#[tokio::test]
async fn test_empty_write_on_closed_writer() {
    let writer_handle = Handle::new(1);

    let writer = Writer::new(writer_handle, "test", Buffer::new());

    // Close the writer
    writer.close().unwrap();

    // Empty write on closed writer should return Err(EBADF)
    let result = writer.write(b"");
    assert_eq!(result, Err(EBADF));
}

#[tokio::test]
async fn test_empty_write_with_errno() {
    let writer_handle = Handle::new(1);

    let writer = Writer::new(writer_handle, "test", Buffer::new());

    // Set error
    writer.set_error(42);

    // Empty write should return Err (error), not Ok(0)
    let result = writer.write(b"");
    assert_eq!(result, Err(42));
    assert_eq!(writer.get_error(), 42);
}

#[tokio::test]
async fn test_reader_dont_read_when_error() {
    let writer_handle = Handle::new(1);
    let writer = Writer::new(writer_handle, "test", Buffer::new());

    let shared_data = writer.share_with_reader();
    let mut reader = Reader::new(Handle::new(2), shared_data);

    // Write some data
    assert_eq!(writer.write(b"hello"), Ok(5));

    // Set reader's own error
    reader.set_error(42);

    // Try to read - should return Err(42) without reading data
    let mut buf = [0u8; 10];
    let result = reader.read(&mut buf).await;
    assert_eq!(result, Err(42));
    assert_eq!(reader.get_error(), 42);

    // Verify data was not read by clearing error and reading
    reader.set_error(0);
    let result = reader.read(&mut buf).await;
    assert_eq!(result, Ok(5));
    assert_eq!(&buf[..5], b"hello");
}

#[tokio::test]
async fn test_reader_get_writer_error() {
    let writer_handle = Handle::new(1);
    let writer = Writer::new(writer_handle, "test", Buffer::new());

    let shared_data = writer.share_with_reader();
    let reader = Reader::new(Handle::new(2), shared_data);

    // Writer sets error
    writer.set_error(99);

    // Reader should see EPIPE, not the writer's raw errno
    assert_eq!(reader.get_error(), EPIPE);
}

#[tokio::test]
async fn test_reader_read_with_writer_error() {
    let writer_handle = Handle::new(1);
    let writer = Writer::new(writer_handle, "test", Buffer::new());

    let shared_data = writer.share_with_reader();
    let mut reader = Reader::new(Handle::new(2), shared_data);

    // Write some data
    assert_eq!(writer.write(b"test"), Ok(4));

    // Reader reads the data successfully
    let mut buf = [0u8; 10];
    let result = reader.read(&mut buf).await;
    assert_eq!(result, Ok(4));
    assert_eq!(&buf[..4], b"test");

    // Writer sets error
    writer.set_error(88);

    // Next read should return Err(EPIPE), not the writer's raw errno
    let result = reader.read(&mut buf).await;
    assert_eq!(result, Err(EPIPE));
    assert_eq!(reader.get_error(), EPIPE);
}

#[tokio::test]
async fn test_reader_drains_buffer_before_error() {
    let writer_handle = Handle::new(1);
    let writer = Writer::new(writer_handle, "test", Buffer::new());

    // Write some data
    assert_eq!(writer.write(b"buffered"), Ok(8));

    // Writer sets error while data is still unread
    writer.set_error(77);

    // Create reader after error is set
    let shared_data = writer.share_with_reader();
    let mut reader = Reader::new(Handle::new(2), shared_data);

    // Reader should still be able to read the buffered data
    let mut buf = [0u8; 10];
    let result = reader.read(&mut buf).await;
    assert_eq!(result, Ok(8));
    assert_eq!(&buf[..8], b"buffered");

    // Now that buffer is drained, next read should return Err(EPIPE)
    let result = reader.read(&mut buf).await;
    assert_eq!(result, Err(EPIPE));
    assert_eq!(reader.get_error(), EPIPE);
}

#[tokio::test]
async fn test_writer_error_notifies_reader() {
    let writer_handle = Handle::new(1);
    let writer = Writer::new(writer_handle, "test", Buffer::new());

    let shared_data = writer.share_with_reader();
    let mut reader = Reader::new(Handle::new(2), shared_data);

    // Spawn reader task that will wait
    let reader_task = tokio::spawn(async move {
        let mut buf = [0u8; 10];
        reader.read(&mut buf).await
    });

    // Writer sets error - should wake the reader
    writer.set_error(55);

    // Reader should wake up with error (Err(EPIPE))
    let result = reader_task.await.unwrap();
    assert_eq!(result, Err(EPIPE));
}

#[tokio::test]
async fn test_reader_own_error_takes_precedence() {
    let writer_handle = Handle::new(1);
    let writer = Writer::new(writer_handle, "test", Buffer::new());

    let shared_data = writer.share_with_reader();
    let mut reader = Reader::new(Handle::new(2), shared_data);

    // Writer sets error
    writer.set_error(5);

    // Reader sets own error
    reader.set_error(10);

    // get_error() should return reader's own error
    assert_eq!(reader.get_error(), 10);

    // read() should return Err with reader's error
    let mut buf = [0u8; 10];
    let result = reader.read(&mut buf).await;
    assert_eq!(result, Err(10));
    assert_eq!(reader.get_error(), 10);
}

// Writer-to-reader EPIPE transformation: reader always sees EPIPE regardless of writer's errno
#[tokio::test]
async fn test_writer_error_transformed_to_epipe() {
    let writer_handle = Handle::new(1);
    let writer = Writer::new(writer_handle, "test", Buffer::new());

    let shared_data = writer.share_with_reader();
    let mut reader = Reader::new(Handle::new(2), shared_data);

    assert_eq!(writer.write(b"data"), Ok(4));
    // Writer closes with EOWNERDEAD — typical actor failure code
    writer.set_error(EOWNERDEAD);
    writer.close().unwrap();

    // Reader drains buffered data first
    let mut buf = [0u8; 10];
    let result = reader.read(&mut buf).await;
    assert_eq!(result, Ok(4));

    // After drain, reader sees EPIPE, not EOWNERDEAD
    let result = reader.read(&mut buf).await;
    assert_eq!(result, Err(EPIPE));
    assert_eq!(reader.get_error(), EPIPE);
}

#[tokio::test]
async fn test_reader_error_checked_before_writer() {
    let writer_handle = Handle::new(1);
    let writer = Writer::new(writer_handle, "test", Buffer::new());

    let shared_data = writer.share_with_reader();
    let mut reader = Reader::new(Handle::new(2), shared_data);

    // Write some data
    assert_eq!(writer.write(b"data"), Ok(4));

    // Reader sets own error first
    reader.set_error(15);

    // Writer sets error after
    writer.set_error(20);

    // Reader should see its own error
    assert_eq!(reader.get_error(), 15);

    // read() should return Err with reader's error
    let mut buf = [0u8; 10];
    let result = reader.read(&mut buf).await;
    assert_eq!(result, Err(15));
}

#[tokio::test]
async fn test_multiple_readers_independent_errors() {
    let writer_handle = Handle::new(1);
    let writer = Writer::new(writer_handle, "test", Buffer::new());

    let shared_data1 = writer.share_with_reader();
    let mut reader1 = Reader::new(Handle::new(2), shared_data1);
    let shared_data2 = writer.share_with_reader();
    let mut reader2 = Reader::new(Handle::new(3), shared_data2);

    // Each reader sets different error
    reader1.set_error(100);
    reader2.set_error(200);

    // Each reader sees its own error
    assert_eq!(reader1.get_error(), 100);
    assert_eq!(reader2.get_error(), 200);

    // Both return Err on read with their own errors
    let mut buf = [0u8; 10];
    assert_eq!(reader1.read(&mut buf).await, Err(100));
    assert_eq!(reader2.read(&mut buf).await, Err(200));
}

#[tokio::test]
async fn test_write_after_all_readers_dropped_gives_epipe() {
    let writer = Writer::new(Handle::new(1), "test", Buffer::new());

    let shared_data = writer.share_with_reader();
    let reader = Reader::new(Handle::new(2), shared_data);

    // Drop the reader so receiver_count == 0
    drop(reader);

    // Writer should detect no readers and return EPIPE
    assert_eq!(writer.write(b"data"), Err(EPIPE));
}
