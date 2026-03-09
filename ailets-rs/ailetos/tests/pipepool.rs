use ailetos::idgen::{Handle, IdGen};
use ailetos::io::memkv::MemKV;
use ailetos::io::KVBuffers;
use ailetos::notification_queue::NotificationQueueArc;
use ailetos::pipepool::PipePool;
use actor_runtime::StdHandle;
use std::sync::Arc;
use std::time::Duration;

// Test helper to create a test pool
fn create_test_pool() -> (PipePool<MemKV>, Arc<MemKV>, Arc<IdGen>) {
    let kv = Arc::new(MemKV::new());
    let queue = NotificationQueueArc::new();
    let id_gen = Arc::new(IdGen::new());
    let pool = PipePool::new(kv.clone(), queue);
    (pool, kv, id_gen)
}

// ============================================================================
// 1. Basic Writer and Reader Creation
// ============================================================================

#[tokio::test]
async fn test_create_writer_then_reader() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Create writer first
    let writer = pool
        .touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to create writer");
    let writer_handle = *writer.handle();

    // Create reader - should succeed immediately since writer exists
    let reader = pool
        .get_or_create_reader((actor_handle, std_handle), false, &id_gen)
        .await;

    assert!(reader.is_some(), "Reader should be created successfully");
    assert_eq!(*reader.unwrap().handle(), Handle::new(id_gen.get_next() - 1));
    assert_eq!(writer_handle.id(), 1);
}

#[tokio::test]
async fn test_multiple_readers_from_same_writer() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Create writer
    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to create output pipe");

    // Create multiple readers
    let reader1 = pool
        .get_or_create_reader((actor_handle, std_handle), false, &id_gen)
        .await;
    let reader2 = pool
        .get_or_create_reader((actor_handle, std_handle), false, &id_gen)
        .await;
    let reader3 = pool
        .get_or_create_reader((actor_handle, std_handle), false, &id_gen)
        .await;

    assert!(reader1.is_some());
    assert!(reader2.is_some());
    assert!(reader3.is_some());

    // Each reader should have a unique handle
    let h1 = *reader1.unwrap().handle();
    let h2 = *reader2.unwrap().handle();
    let h3 = *reader3.unwrap().handle();

    assert_ne!(h1, h2);
    assert_ne!(h2, h3);
    assert_ne!(h1, h3);
}

#[tokio::test]
async fn test_different_std_handles() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);

    // Create writers for different StdHandles
    pool.touch_writer(actor_handle, StdHandle::Stdout, &id_gen)
        .await
        .expect("Failed to create stdout pipe");

    pool.touch_writer(actor_handle, StdHandle::Log, &id_gen)
        .await
        .expect("Failed to create log pipe");

    pool.touch_writer(actor_handle, StdHandle::Env, &id_gen)
        .await
        .expect("Failed to create env pipe");

    // Create readers for each
    let stdout_reader = pool
        .get_or_create_reader((actor_handle, StdHandle::Stdout), false, &id_gen)
        .await;
    let log_reader = pool
        .get_or_create_reader((actor_handle, StdHandle::Log), false, &id_gen)
        .await;
    let env_reader = pool
        .get_or_create_reader((actor_handle, StdHandle::Env), false, &id_gen)
        .await;

    assert!(stdout_reader.is_some());
    assert!(log_reader.is_some());
    assert!(env_reader.is_some());
}

// ============================================================================
// 2. Latent Pipe Functionality (Core Feature)
// ============================================================================

#[tokio::test]
async fn test_create_reader_with_latent_before_writer() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Spawn reader task that will block on latent pipe
    let pool_clone = Arc::new(pool);
    let pool_for_reader = Arc::clone(&pool_clone);
    let id_gen_clone = Arc::clone(&id_gen);

    let reader_task = tokio::spawn(async move {
        pool_for_reader
            .get_or_create_reader((actor_handle, std_handle), true, &id_gen_clone)
            .await
    });

    // Give reader time to start waiting
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Now create the writer - this should notify the waiting reader
    pool_clone
        .touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to realize pipe");

    // Reader should unblock and get a reader
    let result = tokio::time::timeout(Duration::from_secs(1), reader_task)
        .await
        .expect("Reader task timed out")
        .expect("Reader task panicked");

    assert!(result.is_some(), "Reader should be created after writer");
}

#[tokio::test]
async fn test_create_reader_without_latent_returns_none() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Try to create reader with allow_latent=false when no writer exists
    let reader = pool
        .get_or_create_reader((actor_handle, std_handle), false, &id_gen)
        .await;

    assert!(
        reader.is_none(),
        "Reader should be None when no writer exists and allow_latent=false"
    );
}

#[tokio::test]
async fn test_multiple_readers_waiting_on_latent_pipe() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);

    // Spawn multiple reader tasks
    let mut reader_tasks = vec![];
    for _ in 0..3 {
        let pool_clone = Arc::clone(&pool);
        let id_gen_clone = Arc::clone(&id_gen);
        let task = tokio::spawn(async move {
            pool_clone
                .get_or_create_reader((actor_handle, std_handle), true, &id_gen_clone)
                .await
        });
        reader_tasks.push(task);
    }

    // Give readers time to start waiting
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Create writer - should notify all waiting readers
    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to realize pipe");

    // All readers should unblock
    for task in reader_tasks {
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("Reader task timed out")
            .expect("Reader task panicked");

        assert!(result.is_some(), "All readers should be created");
    }
}

#[tokio::test]
async fn test_reader_on_latent_pipe_closed_without_realizing() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);

    // Spawn reader task that will wait on latent pipe
    let pool_for_reader = Arc::clone(&pool);
    let id_gen_clone = Arc::clone(&id_gen);

    let reader_task = tokio::spawn(async move {
        pool_for_reader
            .get_or_create_reader((actor_handle, std_handle), true, &id_gen_clone)
            .await
    });

    // Give reader time to start waiting
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Close the latent writer without realizing it
    pool.close_writer((actor_handle, std_handle));

    // Reader should unblock and get None
    let result = tokio::time::timeout(Duration::from_secs(1), reader_task)
        .await
        .expect("Reader task timed out")
        .expect("Reader task panicked");

    assert!(result.is_none(), "Reader should get None when latent pipe is closed");
}

// ============================================================================
// 3. Latent State Transitions
// ============================================================================

#[tokio::test]
async fn test_latent_to_realized_transition() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Start with no pipe - writer should not exist
    assert!(pool.get_writer((actor_handle, std_handle)).is_none());

    // Create reader with latent - this creates latent writer
    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);
    let pool_clone = Arc::clone(&pool);
    let id_gen_clone = Arc::clone(&id_gen);

    let _reader_task = tokio::spawn(async move {
        pool_clone
            .get_or_create_reader((actor_handle, std_handle), true, &id_gen_clone)
            .await
    });

    // Give time for latent to be created
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Latent pipe exists, but no writer yet
    assert!(pool.get_writer((actor_handle, std_handle)).is_none());

    // Realize the pipe
    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to realize pipe");

    // Now writer exists (realized)
    assert!(pool.get_writer((actor_handle, std_handle)).is_some());

    // Should be able to write to it
    let writer = pool.get_writer((actor_handle, std_handle)).unwrap();
    let result = writer.write(b"test data");
    assert_eq!(result, 9);
}

#[tokio::test]
async fn test_latent_to_closed_transition() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);
    let pool_clone = Arc::clone(&pool);
    let id_gen_clone = Arc::clone(&id_gen);

    // Create latent pipe by requesting reader
    let _reader_task = tokio::spawn(async move {
        pool_clone
            .get_or_create_reader((actor_handle, std_handle), true, &id_gen_clone)
            .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Close without realizing
    pool.close_writer((actor_handle, std_handle));

    // New reader request should get None (closed state)
    let reader = pool
        .get_or_create_reader((actor_handle, std_handle), true, &id_gen)
        .await;

    assert!(reader.is_none(), "Reader should get None from closed latent pipe");
}

// ============================================================================
// 4. Write Operations
// ============================================================================

#[tokio::test]
async fn test_write_to_realized_writer() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Create writer
    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to create output pipe");

    // Get writer
    let writer = pool.get_writer((actor_handle, std_handle));
    assert!(writer.is_some());
    let writer = writer.unwrap();

    // Write data
    let result = writer.write(b"Hello World");
    assert_eq!(result, 11);

    // Write more data
    let result = writer.write(b"123");
    assert_eq!(result, 3);
}

#[tokio::test]
async fn test_write_to_nonexistent_pipe() {
    let (pool, _, _id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Try to get writer without creating pipe
    let writer = pool.get_writer((actor_handle, std_handle));
    assert!(writer.is_none(), "get_writer on non-existent pipe should return None");
}

#[tokio::test]
async fn test_multiple_writes_to_same_pipe() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to create output pipe");

    // Get writer
    let writer = pool.get_writer((actor_handle, std_handle)).unwrap();

    // Multiple writes
    for i in 0..10 {
        let data = format!("data{}", i);
        let result = writer.write(data.as_bytes());
        assert_eq!(result, data.len() as isize);
    }
}

// ============================================================================
// 5. Close Operations
// ============================================================================

#[tokio::test]
async fn test_close_realized_writer() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to create output pipe");

    // Close the writer
    pool.close_writer((actor_handle, std_handle));

    // Writer should still exist (close doesn't remove it)
    assert!(pool.get_writer((actor_handle, std_handle)).is_some());
}

#[tokio::test]
async fn test_close_latent_writer_without_realizing() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);
    let pool_clone = Arc::clone(&pool);
    let id_gen_clone = Arc::clone(&id_gen);

    // Create latent pipe
    let _reader_task = tokio::spawn(async move {
        pool_clone
            .get_or_create_reader((actor_handle, std_handle), true, &id_gen_clone)
            .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Close latent writer (abnormal case - should log warning)
    pool.close_writer((actor_handle, std_handle));

    // Writer still doesn't exist (latent was closed)
    assert!(pool.get_writer((actor_handle, std_handle)).is_none());
}

#[tokio::test]
async fn test_close_nonexistent_pipe() {
    let (pool, _, _id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Close non-existent pipe - should be no-op
    pool.close_writer((actor_handle, std_handle));

    // Writer still doesn't exist
    assert!(pool.get_writer((actor_handle, std_handle)).is_none());
}

#[tokio::test]
async fn test_multiple_close_calls() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to create output pipe");

    // Close multiple times - should be idempotent
    pool.close_writer((actor_handle, std_handle));
    pool.close_writer((actor_handle, std_handle));
    pool.close_writer((actor_handle, std_handle));

    // Writer should still exist (close doesn't remove it)
    assert!(pool.get_writer((actor_handle, std_handle)).is_some());
}

// ============================================================================
// 6. Pipe Existence Checks
// ============================================================================


// ============================================================================
// 7. Concurrent Operations
// ============================================================================

#[tokio::test]
async fn test_multiple_readers_waiting_concurrently() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);

    // Spawn 5 concurrent reader tasks
    let mut tasks = vec![];
    for _ in 0..5 {
        let pool_clone = Arc::clone(&pool);
        let id_gen_clone = Arc::clone(&id_gen);
        let task = tokio::spawn(async move {
            pool_clone
                .get_or_create_reader((actor_handle, std_handle), true, &id_gen_clone)
                .await
        });
        tasks.push(task);
    }

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Realize the pipe
    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to realize pipe");

    // All tasks should complete successfully
    for task in tasks {
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("Task timed out")
            .expect("Task panicked");
        assert!(result.is_some());
    }
}

#[tokio::test]
async fn test_concurrent_writers_for_different_handles() {
    let (pool, _, id_gen) = create_test_pool();
    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);

    // Create multiple writers concurrently for different handles
    let mut tasks = vec![];
    for i in 1..=5 {
        let pool_clone = Arc::clone(&pool);
        let id_gen_clone = Arc::clone(&id_gen);
        let task = tokio::spawn(async move {
            let actor_handle = Handle::new(i);
            pool_clone
                .touch_writer(actor_handle, StdHandle::Stdout, &id_gen_clone)
                .await
        });
        tasks.push(task);
    }

    // All should succeed
    for task in tasks {
        let result = task.await.expect("Task panicked");
        assert!(result.is_ok());
    }

    // All writers should exist
    for i in 1..=5 {
        assert!(pool.get_writer((Handle::new(i), StdHandle::Stdout)).is_some());
    }
}

// ============================================================================
// 8. Integration with KV Storage
// ============================================================================

#[tokio::test]
async fn test_create_output_pipe_allocates_buffer() {
    let (pool, kv, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to create writer");

    // Buffer should be allocated in KV
    let buffer_name = format!("pipes/actor-{}-{:?}", actor_handle.id(), std_handle);
    let buffer = kv.open(&buffer_name, ailetos::io::OpenMode::Read).await;
    assert!(buffer.is_ok(), "Buffer should exist in KV storage");
}

#[tokio::test]
async fn test_realize_pipe_allocates_buffer() {
    let (pool, kv, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to realize pipe");

    // Buffer should exist
    let buffer_name = format!("pipes/actor-{}-{:?}", actor_handle.id(), std_handle);
    let buffer = kv.open(&buffer_name, ailetos::io::OpenMode::Read).await;
    assert!(buffer.is_ok(), "Buffer should exist in KV storage");
}

#[tokio::test]
async fn test_realize_already_realized_is_noop() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Realize once
    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to realize pipe first time");

    // Realize again - should be no-op
    let result = pool
        .touch_writer(actor_handle, std_handle, &id_gen)
        .await;
    assert!(result.is_ok(), "Second realize should succeed (no-op)");
}

#[tokio::test]
async fn test_flush_buffer() {
    let (pool, kv, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to create output pipe");

    // Write some data
    let writer = pool.get_writer((actor_handle, std_handle)).unwrap();
    writer.write(b"test data");

    // Flush should succeed
    let result = kv.flush_buffer(&pool.get_writer((actor_handle, std_handle)).unwrap().buffer()).await;
    assert!(result.is_ok(), "Flush should succeed");
}

// ============================================================================
// 9. Error Handling
// ============================================================================

#[tokio::test]
async fn test_touch_writer_idempotent() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Create first time - should succeed
    let writer1 = pool
        .touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("First create should succeed");

    // Create second time - should succeed and return same writer (idempotent)
    let writer2 = pool
        .touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Second create should succeed (idempotent)");

    // Both should be the same writer (same handle)
    assert_eq!(writer1.handle(), writer2.handle(), "Should return same writer when called twice");
}

#[tokio::test]
async fn test_flush_buffer_on_nonexistent_pipe() {
    let (pool, _, _id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Try to get writer for non-existent pipe - should return None
    let writer = pool.get_writer((actor_handle, std_handle));
    assert!(writer.is_none(), "Getting writer for non-existent pipe should return None");
}

// ============================================================================
// 10. Edge Cases
// ============================================================================

#[tokio::test]
async fn test_reader_waits_indefinitely_until_resolved() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);
    let pool_clone = Arc::clone(&pool);
    let id_gen_clone = Arc::clone(&id_gen);

    // Start reader task
    let reader_task = tokio::spawn(async move {
        pool_clone
            .get_or_create_reader((actor_handle, std_handle), true, &id_gen_clone)
            .await
    });

    // Wait longer than normal
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Task should still be waiting (not completed)
    assert!(!reader_task.is_finished());

    // Now realize the pipe
    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to realize pipe");

    // Task should complete
    let result = tokio::time::timeout(Duration::from_secs(1), reader_task)
        .await
        .expect("Task should complete")
        .expect("Task panicked");

    assert!(result.is_some());
}

#[tokio::test]
async fn test_create_latent_then_realize_then_another_reader() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);

    // Create latent pipe via reader
    let pool_clone = Arc::clone(&pool);
    let id_gen_clone = Arc::clone(&id_gen);
    let reader1_task = tokio::spawn(async move {
        pool_clone
            .get_or_create_reader((actor_handle, std_handle), true, &id_gen_clone)
            .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Realize the pipe
    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to realize pipe");

    // Wait for first reader
    reader1_task.await.expect("First reader panicked");

    // Create another reader - should succeed immediately
    let reader2 = pool
        .get_or_create_reader((actor_handle, std_handle), false, &id_gen)
        .await;

    assert!(reader2.is_some(), "Second reader should be created immediately");
}

#[tokio::test]
async fn test_mixed_latent_and_realized_pipes() {
    let (pool, _, id_gen) = create_test_pool();
    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);

    // Create realized pipe for actor 1
    pool.touch_writer(Handle::new(1), StdHandle::Stdout, &id_gen)
        .await
        .expect("Failed to create pipe 1");

    // Create latent pipe for actor 2
    let pool_clone = Arc::clone(&pool);
    let id_gen_clone = Arc::clone(&id_gen);
    let _reader_task = tokio::spawn(async move {
        pool_clone
            .get_or_create_reader((Handle::new(2), StdHandle::Stdout), true, &id_gen_clone)
            .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Get writer for realized pipe should work
    let writer = pool.get_writer((Handle::new(1), StdHandle::Stdout));
    assert!(writer.is_some());
    writer.unwrap().write(b"data");

    // Get writer for latent pipe should fail (not yet realized)
    let writer = pool.get_writer((Handle::new(2), StdHandle::Stdout));
    assert!(writer.is_none());
}

// ============================================================================
// 11. Real-world Scenarios
// ============================================================================

#[tokio::test]
async fn test_attachment_workflow_simulation() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);

    // Simulate attachment: create reader before actor writes
    let pool_clone = Arc::clone(&pool);
    let id_gen_clone = Arc::clone(&id_gen);
    let attachment_task = tokio::spawn(async move {
        let mut reader = pool_clone
            .get_or_create_reader((actor_handle, std_handle), true, &id_gen_clone)
            .await
            .expect("Attachment should get reader");

        // Read data (simulated)
        let mut buf = vec![0u8; 100];
        let mut total_read = 0;
        loop {
            let n = reader.read(&mut buf).await;
            if n > 0 {
                total_read += n;
            } else {
                break;
            }
        }
        total_read
    });

    // Give attachment time to start waiting
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Actor starts and writes
    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to realize pipe");

    let writer = pool.get_writer((actor_handle, std_handle)).unwrap();
    writer.write(b"Hello from actor!");

    // Actor closes
    pool.close_writer((actor_handle, std_handle));

    // Attachment should complete
    let bytes_read = tokio::time::timeout(Duration::from_secs(1), attachment_task)
        .await
        .expect("Attachment should complete")
        .expect("Attachment panicked");

    assert!(bytes_read > 0, "Attachment should read data");
}

#[tokio::test]
async fn test_dependency_reading_simulation() {
    let (pool, _, id_gen) = create_test_pool();
    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);

    // Simulate a pipeline: actor1 -> actor2
    let actor1 = Handle::new(1);

    // Actor2 starts reading from actor1's stdout (dependency)
    let pool_clone = Arc::clone(&pool);
    let id_gen_clone = Arc::clone(&id_gen);
    let reader_task = tokio::spawn(async move {
        pool_clone
            .get_or_create_reader((actor1, StdHandle::Stdout), true, &id_gen_clone)
            .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Actor1 starts and writes
    pool.touch_writer(actor1, StdHandle::Stdout, &id_gen)
        .await
        .expect("Failed to realize actor1 pipe");

    let writer = pool.get_writer((actor1, StdHandle::Stdout)).unwrap();
    writer.write(b"data from actor1");

    // Reader should get the reader
    let reader = tokio::time::timeout(Duration::from_secs(1), reader_task)
        .await
        .expect("Reader should unblock")
        .expect("Reader task panicked");

    assert!(reader.is_some(), "Dependency reader should be created");

    // Read the data
    let mut buf = vec![0u8; 100];
    let n = reader.unwrap().read(&mut buf).await;
    assert_eq!(n, 16);
    assert_eq!(&buf[..16], b"data from actor1");
}

#[tokio::test]
async fn test_end_to_end_data_flow() {
    let (pool, _, id_gen) = create_test_pool();
    let actor_handle = Handle::new(1);
    let std_handle = StdHandle::Stdout;

    // Create reader first (latent)
    let pool = Arc::new(pool);
    let id_gen = Arc::new(id_gen);
    let pool_for_reader = Arc::clone(&pool);
    let id_gen_clone = Arc::clone(&id_gen);

    let reader_task = tokio::spawn(async move {
        let mut reader = pool_for_reader
            .get_or_create_reader((actor_handle, std_handle), true, &id_gen_clone)
            .await
            .expect("Should get reader");

        let mut all_data = Vec::new();
        let mut buf = vec![0u8; 50];

        loop {
            let n = reader.read(&mut buf).await;
            if n > 0 {
                all_data.extend_from_slice(&buf[..n as usize]);
            } else {
                break;
            }
        }
        all_data
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Realize pipe
    pool.touch_writer(actor_handle, std_handle, &id_gen)
        .await
        .expect("Failed to realize pipe");

    // Write multiple chunks
    let writer = pool.get_writer((actor_handle, std_handle)).unwrap();
    writer.write(b"First ");
    writer.write(b"Second ");
    writer.write(b"Third");

    // Close writer
    pool.close_writer((actor_handle, std_handle));

    // Reader should get all data
    let data = tokio::time::timeout(Duration::from_secs(1), reader_task)
        .await
        .expect("Reader should complete")
        .expect("Reader panicked");

    assert_eq!(data, b"First Second Third");
}
