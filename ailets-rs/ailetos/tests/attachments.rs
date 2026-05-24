//! Tests for pipe fan-out behavior

use std::sync::{Arc, Mutex};

use actor_runtime::StdHandle;
use ailetos::pipe::{copy_to_writer, FlushMode};
use ailetos::{Environment, Executor, KVBuffers, MemKV};

/// Simple writer that captures all writes to a Vec<u8>
struct CaptureWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl CaptureWriter {
    fn new() -> (Self, Arc<Mutex<Vec<u8>>>) {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        (
            Self {
                buffer: Arc::clone(&buffer),
            },
            buffer,
        )
    }
}

impl std::io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Test that each reader gets its own independent reader that can read at its own pace.
/// This verifies the fan-out behavior where multiple PipePool readers read from the same
/// pipe independently without interfering with each other.
#[tokio::test]
async fn test_fanout_independent_readers() {
    let kv: Arc<dyn KVBuffers> = Arc::new(MemKV::new());
    let env = Arc::new(Environment::new(kv));

    env.actor_registry.write().register("chunked_writer", |rt| {
        use actor_io::AWriter;
        use actor_runtime::StdHandle;
        use embedded_io::Write;

        let mut writer = AWriter::new_from_std(rt, StdHandle::Stdout);
        writer
            .write_all(b"chunk1")
            .map_err(|e| format!("write failed: {e:?}"))?;
        writer
            .write_all(b"chunk2")
            .map_err(|e| format!("write failed: {e:?}"))?;
        writer
            .write_all(b"chunk3")
            .map_err(|e| format!("write failed: {e:?}"))?;
        Ok(())
    });

    let node = env.add_node("chunked_writer".to_string(), &[], Some("test".to_string()));

    let fd = StdHandle::Stdout as isize;
    let (sink1, buffer1) = CaptureWriter::new();
    let (sink2, buffer2) = CaptureWriter::new();
    let (sink3, buffer3) = CaptureWriter::new();

    let task1 = {
        let pool = Arc::clone(&env.pipe_pool);
        let gen = Arc::clone(&env.idgen);
        tokio::spawn(async move {
            if let Ok(reader) = pool.get_or_await_new_reader((node, fd), true, &gen).await {
                let _ = copy_to_writer(reader, sink1, FlushMode::AfterEachWrite).await;
            }
        })
    };
    let task2 = {
        let pool = Arc::clone(&env.pipe_pool);
        let gen = Arc::clone(&env.idgen);
        tokio::spawn(async move {
            if let Ok(reader) = pool.get_or_await_new_reader((node, fd), true, &gen).await {
                let _ = copy_to_writer(reader, sink2, FlushMode::AfterEachWrite).await;
            }
        })
    };
    let task3 = {
        let pool = Arc::clone(&env.pipe_pool);
        let gen = Arc::clone(&env.idgen);
        tokio::spawn(async move {
            if let Ok(reader) = pool.get_or_await_new_reader((node, fd), true, &gen).await {
                let _ = copy_to_writer(reader, sink3, FlushMode::AfterEachWrite).await;
            }
        })
    };

    let executor = Executor::start(tokio::runtime::Handle::current(), Arc::clone(&env), None);
    executor.submit(node, Default::default()).expect("submit failed");
    executor.shutdown().await;

    task1.await.unwrap();
    task2.await.unwrap();
    task3.await.unwrap();

    assert_eq!(*buffer1.lock().unwrap(), b"chunk1chunk2chunk3");
    assert_eq!(*buffer2.lock().unwrap(), b"chunk1chunk2chunk3");
    assert_eq!(*buffer3.lock().unwrap(), b"chunk1chunk2chunk3");
}
