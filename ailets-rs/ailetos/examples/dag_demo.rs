use std::sync::Arc;

use ailetos::{Dag, DependsOn, For, IdGen, NodeKind, NodeState};

fn main() {
    let idgen = Arc::new(IdGen::new());
    let mut dag = Dag::new(idgen.clone());

    // Create a structure similar to the Python example
    let value_13 = dag.add_node("value.13 (chat messages)".to_string(), NodeKind::Concrete);
    dag.set_state(value_13, NodeState::Terminated).unwrap();

    let messages_to_query_15 = dag.add_node("gpt.messages_to_query.15".to_string(), NodeKind::Concrete);
    dag.add_dependency(For(messages_to_query_15), DependsOn(value_13));

    let query_16 = dag.add_node("query.16".to_string(), NodeKind::Concrete);
    dag.add_dependency(For(query_16), DependsOn(messages_to_query_15));

    let response_to_messages_17 = dag.add_node("gpt.response_to_messages.17".to_string(), NodeKind::Concrete);
    dag.add_dependency(For(response_to_messages_17), DependsOn(query_16));

    let messages_to_markdown_18 = dag.add_node("messages_to_markdown.18".to_string(), NodeKind::Concrete);
    dag.add_dependency(For(messages_to_markdown_18), DependsOn(response_to_messages_17));

    println!("Dependency tree:");
    print!("{}", dag.dump(messages_to_markdown_18));

    println!("\n\nDiamond dependency structure:");
    let mut dag2 = Dag::new(idgen.clone());

    let d = dag2.add_node("D".to_string(), NodeKind::Concrete);
    dag2.set_state(d, NodeState::Terminated).unwrap();

    let b = dag2.add_node("B".to_string(), NodeKind::Concrete);
    dag2.set_state(b, NodeState::Running).unwrap();
    dag2.add_dependency(For(b), DependsOn(d));

    let c = dag2.add_node("C".to_string(), NodeKind::Concrete);
    dag2.add_dependency(For(c), DependsOn(d));

    let a = dag2.add_node("A".to_string(), NodeKind::Concrete);
    dag2.add_dependency(For(a), DependsOn(b));
    dag2.add_dependency(For(a), DependsOn(c));

    print!("{}", dag2.dump(a));

    println!("\n\nAlias resolution (aliases are not shown):");
    let mut dag3 = Dag::new(idgen.clone());

    let node1 = dag3.add_node("concrete_node_1".to_string(), NodeKind::Concrete);
    dag3.set_state(node1, NodeState::Terminated).unwrap();

    let node2 = dag3.add_node("concrete_node_2".to_string(), NodeKind::Concrete);
    dag3.set_state(node2, NodeState::Terminated).unwrap();

    let alias = dag3.add_node("my_alias".to_string(), NodeKind::Alias);
    dag3.add_dependency(For(alias), DependsOn(node1));
    dag3.add_dependency(For(alias), DependsOn(node2));

    let root = dag3.add_node("root".to_string(), NodeKind::Concrete);
    dag3.add_dependency(For(root), DependsOn(alias));

    print!("{}", dag3.dump(root));
    println!("(Notice: 'my_alias' is not shown, only the concrete nodes it refers to)");
}
