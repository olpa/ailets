use std::sync::Arc;

use ailetos::pipe::pipe_path;
use ailetos::{Environment, Handle, KVBuffers, MemKV, NodeKind, OpenMode};
use dagsh::prompt_nodes::{register_prompt_inputs, StdinUsage};
use dagsh::shell_ui::PromptArg;

fn make_env() -> (Arc<Environment>, tokio::runtime::Runtime) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let kv = Arc::new(MemKV::new());
    let env = Arc::new(Environment::new(Arc::clone(&kv) as Arc<dyn KVBuffers>));
    (env, rt)
}

fn alias_deps(env: &Arc<Environment>, alias: &str) -> Vec<Handle> {
    let dag = env.dag.read();
    let node = dag
        .nodes()
        .find(|n| n.kind == NodeKind::Alias && n.idname == alias)
        .unwrap_or_else(|| panic!("{alias} alias not found"));
    dag.get_direct_dependencies(node.pid).collect()
}

fn read_value_node(
    env: &Arc<Environment>,
    rt: &tokio::runtime::Runtime,
    handle: Handle,
) -> String {
    let path = pipe_path(handle, actor_runtime::StdHandle::Stdout as isize);
    let kv = Arc::clone(&env.kv);
    rt.block_on(async move {
        let buffer = kv.open(&path, OpenMode::Read).await.unwrap();
        let guard = buffer.lock();
        String::from_utf8(guard.to_vec()).unwrap()
    })
}

fn node_idname(env: &Arc<Environment>, handle: Handle) -> String {
    env.dag.read().get_node(handle).unwrap().idname.clone()
}

fn node_explain(env: &Arc<Environment>, handle: Handle) -> Option<String> {
    env.dag.read().get_node(handle).unwrap().explain.clone()
}

fn to_doc_item_dep(env: &Arc<Environment>, handle: Handle) -> Handle {
    let dag = env.dag.read();
    let deps: Vec<Handle> = dag.get_direct_dependencies(handle).collect();
    assert_eq!(deps.len(), 1, "to_doc_item should have exactly one dep");
    deps[0]
}

// test 8: ctl(user) auto-inserted once before first non-SystemPrompt item;
//         subsequent items do not trigger a second ctl(user)
#[test]
fn test_ctl_user_inserted_once() {
    let (env, rt) = make_env();
    let items = vec![
        PromptArg::Text("Hello".to_string()),
        PromptArg::Text("World".to_string()),
    ];
    register_prompt_inputs(&env, rt.handle(), &items).unwrap();

    let doc_deps = alias_deps(&env, "input_doc");
    assert_eq!(doc_deps.len(), 3, "expected ctl(user) + to_doc_item x2");
    assert_eq!(
        read_value_node(&env, &rt, doc_deps[0]),
        r#"[{"type":"ctl"},{"role":"user"}]"#
    );
    assert_eq!(node_idname(&env, doc_deps[1]), "to_doc_item");
    assert_eq!(node_idname(&env, doc_deps[2]), "to_doc_item");

    let raw_deps = alias_deps(&env, "input_raw");
    assert_eq!(raw_deps.len(), 2, "expected two raw value nodes");
    assert_eq!(read_value_node(&env, &rt, raw_deps[0]), "Hello");
    assert_eq!(read_value_node(&env, &rt, raw_deps[1]), "World");

    // each to_doc_item depends on its corresponding raw node
    assert_eq!(to_doc_item_dep(&env, doc_deps[1]), raw_deps[0]);
    assert_eq!(to_doc_item_dep(&env, doc_deps[2]), raw_deps[1]);
}

// test 9: SystemPrompt interleaved mid-sequence → ctl(system)+text at that
//         position, not hoisted to the front
#[test]
fn test_system_prompt_interleaved() {
    let (env, rt) = make_env();
    let items = vec![
        PromptArg::Text("Hello".to_string()),
        PromptArg::SystemPrompt("Be formal".to_string()),
        PromptArg::Text("World".to_string()),
    ];
    register_prompt_inputs(&env, rt.handle(), &items).unwrap();

    let doc_deps = alias_deps(&env, "input_doc");
    // ctl(user) + to_doc_item(Hello) + ctl(system) + to_doc_item(Be formal)
    // + ctl(user) + to_doc_item(World)
    assert_eq!(doc_deps.len(), 6);
    assert_eq!(read_value_node(&env, &rt, doc_deps[0]), r#"[{"type":"ctl"},{"role":"user"}]"#);
    assert_eq!(node_idname(&env, doc_deps[1]), "to_doc_item");
    assert_eq!(read_value_node(&env, &rt, doc_deps[2]), r#"[{"type":"ctl"},{"role":"system"}]"#);
    assert_eq!(node_idname(&env, doc_deps[3]), "to_doc_item");
    assert_eq!(read_value_node(&env, &rt, doc_deps[4]), r#"[{"type":"ctl"},{"role":"user"}]"#);
    assert_eq!(node_idname(&env, doc_deps[5]), "to_doc_item");

    let raw_deps = alias_deps(&env, "input_raw");
    assert_eq!(raw_deps.len(), 3);
    assert_eq!(read_value_node(&env, &rt, raw_deps[0]), "Hello");
    assert_eq!(read_value_node(&env, &rt, raw_deps[1]), "Be formal");
    assert_eq!(read_value_node(&env, &rt, raw_deps[2]), "World");
}

// consecutive system prompts share a single ctl(system) node
#[test]
fn test_consecutive_system_prompts_share_ctl() {
    let (env, rt) = make_env();
    let items = vec![
        PromptArg::SystemPrompt("EE".to_string()),
        PromptArg::SystemPrompt("FF".to_string()),
        PromptArg::Text("hello".to_string()),
    ];
    register_prompt_inputs(&env, rt.handle(), &items).unwrap();

    let doc_deps = alias_deps(&env, "input_doc");
    // ctl(system) + to_doc_item(EE) + to_doc_item(FF) + ctl(user) + to_doc_item(hello)
    assert_eq!(doc_deps.len(), 5);
    assert_eq!(read_value_node(&env, &rt, doc_deps[0]), r#"[{"type":"ctl"},{"role":"system"}]"#);
    assert_eq!(node_idname(&env, doc_deps[1]), "to_doc_item");
    assert_eq!(node_idname(&env, doc_deps[2]), "to_doc_item");
    assert_eq!(read_value_node(&env, &rt, doc_deps[3]), r#"[{"type":"ctl"},{"role":"user"}]"#);
    assert_eq!(node_idname(&env, doc_deps[4]), "to_doc_item");

    let raw_deps = alias_deps(&env, "input_raw");
    assert_eq!(raw_deps.len(), 3);
    assert_eq!(read_value_node(&env, &rt, raw_deps[0]), "EE");
    assert_eq!(read_value_node(&env, &rt, raw_deps[1]), "FF");
    assert_eq!(read_value_node(&env, &rt, raw_deps[2]), "hello");
}

// test 10a: explicit Stdin stays at its position in the sequence
#[test]
fn test_explicit_stdin_stays_at_position() {
    let (env, rt) = make_env();
    let items = vec![
        PromptArg::Stdin,
        PromptArg::Text("Hello".to_string()),
    ];
    let consumed = register_prompt_inputs(&env, rt.handle(), &items).unwrap();

    assert_eq!(consumed, StdinUsage::FileValue, "stdin should be marked consumed");

    let doc_deps = alias_deps(&env, "input_doc");
    // ctl(user) + to_doc_item(stdin) + to_doc_item(Hello)
    assert_eq!(doc_deps.len(), 3);
    assert_eq!(read_value_node(&env, &rt, doc_deps[0]), r#"[{"type":"ctl"},{"role":"user"}]"#);
    assert_eq!(node_idname(&env, doc_deps[1]), "to_doc_item");
    assert_eq!(node_idname(&env, doc_deps[2]), "to_doc_item");

    let raw_deps = alias_deps(&env, "input_raw");
    // file_value(stdin) + raw_text(Hello)
    assert_eq!(raw_deps.len(), 2);
    assert_eq!(node_idname(&env, raw_deps[0]), "file_value");
    assert_eq!(node_explain(&env, raw_deps[0]).as_deref(), Some("-"));
    assert_eq!(read_value_node(&env, &rt, raw_deps[1]), "Hello");

    assert_eq!(to_doc_item_dep(&env, doc_deps[1]), raw_deps[0]);
    assert_eq!(to_doc_item_dep(&env, doc_deps[2]), raw_deps[1]);
}

