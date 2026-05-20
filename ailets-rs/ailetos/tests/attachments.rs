//! Tests for attachment fan-out behavior

use std::sync::{Arc, Mutex};

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

/// Test that each sink gets its own independent reader that can read at its own pace.
/// This verifies the fan-out behavior where multiple attachments read from the same
/// pipe independently without interfering with each other.
#[tokio::test]
async fn test_fanout_independent_readers() {
    // Setup environment and executor
    let kv: Arc<dyn KVBuffers> = Arc::new(MemKV::new());
    let env = Arc::new(Environment::new(kv));

    // Register an actor that writes multiple chunks
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

    // Attach THREE independent sinks to the same node to test fan-out
    let (sink1, buffer1) = CaptureWriter::new();
    let (sink2, buffer2) = CaptureWriter::new();
    let (sink3, buffer3) = CaptureWriter::new();

    env.attach_stdout_to(node, Box::new(sink1));
    env.attach_stdout_to(node, Box::new(sink2));
    env.attach_stdout_to(node, Box::new(sink3));

    // Run the executor
    let executor = Executor::start(Arc::clone(&env), None);
    executor
        .submit(node, Default::default())
        .expect("submit failed");
    executor.shutdown().await;

    // All three readers should have independently received all chunks
    let data1 = buffer1.lock().unwrap().clone();
    let data2 = buffer2.lock().unwrap().clone();
    let data3 = buffer3.lock().unwrap().clone();

    assert_eq!(data1, b"chunk1chunk2chunk3");
    assert_eq!(data2, b"chunk1chunk2chunk3");
    assert_eq!(data3, b"chunk1chunk2chunk3");
}
