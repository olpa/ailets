use std::sync::Arc;

use ailetos::dag::NodeState;
use ailetos::pipe::pipe_path;
use ailetos::storage::{KVBuffers, MemKV, OpenMode};
use ailetos::Environment;

#[tokio::test]
async fn test_value_node_is_built_at_creation() {
    let kv = Arc::new(MemKV::new());
    let mut env = Environment::new(kv);

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
    let kv = Arc::new(MemKV::new());
    let mut env = Environment::new(Arc::clone(&kv));

    let test_data = b"immediate value data";
    let handle = env
        .add_value_node(test_data.to_vec(), Some("Test immediate value".to_string()))
        .await
        .expect("Failed to add value node");

    // Verify data was written to KV storage
    let path = pipe_path(handle, actor_runtime::StdHandle::Stdout);
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
