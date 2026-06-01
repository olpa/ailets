use std::sync::Arc;

use ailetos::dag::NodeState;
use ailetos::pipe::pipe_path;
use ailetos::storage::{KVBuffers, MemKV, OpenMode};
use ailetos::{Environment, NodeKind};

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

#[test]
fn test_add_alias_single_target() {
    let kv: Arc<dyn KVBuffers> = Arc::new(MemKV::new());
    let env = Environment::new(kv);

    let a = env.add_node("a".to_string(), &[], None);
    let b = env.add_node("b".to_string(), &[], None);

    let alias_a = env.add_alias(".end".to_string(), a);
    let alias_b = env.add_alias(".end".to_string(), b);

    // Same alias name reuses the same node
    assert_eq!(alias_a, alias_b);

    // The alias resolves to both targets
    let mut resolved = env.resolve_all(alias_a);
    resolved.sort_by_key(|h| h.id());
    let mut expected = vec![a, b];
    expected.sort_by_key(|h| h.id());
    assert_eq!(resolved, expected);

    // Check that the alias node is recorded as Alias kind
    let dag = env.dag.read();
    assert_eq!(dag.get_node(alias_a).unwrap().kind, NodeKind::Alias);
}
