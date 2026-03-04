use std::sync::Arc;

use ailetos::dag::NodeState;
use ailetos::io::MemKV;
use ailetos::Environment;

#[test]
fn test_value_node_is_built_at_creation() {
    let kv = Arc::new(MemKV::new());
    let mut env = Environment::new(kv);

    let handle = env.add_value_node(b"test data".to_vec(), Some("Test value".to_string()));

    let node = env.dag.get_node(handle).expect("Node should exist");
    assert_eq!(
        node.state,
        NodeState::Terminated,
        "Value node should be marked as built (Terminated) at creation"
    );
}
