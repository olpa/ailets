use std::sync::{Arc, Mutex};

use actor_runtime::StdHandle;
use ailetos::dag::NodeState;
use ailetos::pipe::{copy_to_writer, pipe_path, FlushMode};
use ailetos::storage::{KVBuffers, MemKV, OpenMode};
use ailetos::{Environment, Executor};
use ailetos::traversal::StopConditions;

// ---------------------------------------------------------------------------
// Collecting sink for testing output capture
// ---------------------------------------------------------------------------

struct CollectingSink(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for CollectingSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn two_follows_both_receive_output() {
    let received1: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let received2: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

    let kv: Arc<dyn KVBuffers> = Arc::new(MemKV::new());
    let env = Arc::new(Environment::new(kv));
    env.actor_registry.write().register("cat", cat::execute);

    let val = env.add_value_node(b"hello".to_vec(), None).await.unwrap();
    let cat_node = env.add_node("cat".to_string(), &[val], None);

    let fd = StdHandle::Stdout as isize;

    let task1 = {
        let pool = Arc::clone(&env.pipe_pool);
        let gen = Arc::clone(&env.idgen);
        let sink = Arc::clone(&received1);
        tokio::spawn(async move {
            if let Ok(reader) = pool.get_or_await_new_reader((cat_node, fd), true, &gen).await {
                let _ = copy_to_writer(reader, CollectingSink(sink), FlushMode::AfterEachWrite).await;
            }
        })
    };

    let task2 = {
        let pool = Arc::clone(&env.pipe_pool);
        let gen = Arc::clone(&env.idgen);
        let sink = Arc::clone(&received2);
        tokio::spawn(async move {
            if let Ok(reader) = pool.get_or_await_new_reader((cat_node, fd), true, &gen).await {
                let _ = copy_to_writer(reader, CollectingSink(sink), FlushMode::AfterEachWrite).await;
            }
        })
    };

    let executor = Executor::start(tokio::runtime::Handle::current(), Arc::clone(&env), None);
    executor.submit(cat_node, StopConditions::default()).unwrap();
    executor.shutdown().await;

    task1.await.unwrap();
    task2.await.unwrap();

    assert_eq!(received1.lock().unwrap().as_slice(), b"hello");
    assert_eq!(received2.lock().unwrap().as_slice(), b"hello");
}

#[tokio::test]
async fn test_value_node_is_built_at_creation() {
    let kv: Arc<dyn KVBuffers> = Arc::new(MemKV::new());
    let env = Environment::new(kv);

    let handle = env
        .add_value_node(b"test data".to_vec(), Some("Test value".to_string()))
        .await
        .expect("Failed to add value node");

    let dag = env.dag.read();
    let node = dag.get_node(handle).expect("Node should exist");
    assert_eq!(
        node.state,
        NodeState::Terminated,
        "Value node should be marked as built (Terminated) at creation"
    );
}

#[tokio::test]
async fn test_value_node_writes_data_to_kv() {
    let kv: Arc<dyn KVBuffers> = Arc::new(MemKV::new());
    let env = Environment::new(Arc::clone(&kv));

    let test_data = b"immediate value data";
    let handle = env
        .add_value_node(test_data.to_vec(), Some("Test immediate value".to_string()))
        .await
        .expect("Failed to add value node");

    // Verify data was written to KV storage
    let path = pipe_path(handle, actor_runtime::StdHandle::Stdout as isize);
    let buffer = kv
        .open(&path, OpenMode::Read)
        .await
        .expect("KV buffer should exist");

    let data = buffer.lock();
    assert_eq!(
        &*data, test_data,
        "Value node data should be written to KV storage immediately"
    );
}
