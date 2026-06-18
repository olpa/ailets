//! Build DAG nodes and aliases from parsed prompt items, and session dispatch.

use std::sync::Arc;

use ailetos::{Environment, Handle};

use crate::shell_ui::PromptArg;
use file_value::control as file_value_control;
use to_doc_item::control as to_doc_item_control;

const CTL_USER_JSON: &[u8] = br#"[{"type":"ctl"},{"role":"user"}]"#;
const CTL_SYSTEM_JSON: &[u8] = br#"[{"type":"ctl"},{"role":"system"}]"#;

/// Creates a value node from `data` and adds it to the `"input_doc"` alias.
/// Used for CTL role-marker nodes, which are already structured.
fn add_ctl_to_input_doc(
    env: &Arc<Environment>,
    async_runtime: &tokio::runtime::Handle,
    data: &[u8],
) -> Result<(), String> {
    let env_clone = Arc::clone(env);
    let data = data.to_vec();
    let handle = async_runtime
        .block_on(async move { env_clone.add_value_node(data, None).await })
        .map_err(|e| format!("failed to add ctl value node: {e}"))?;
    let _h = env.add_alias("input_doc".to_string(), handle);
    Ok(())
}

/// Creates a raw value node from `data`, adds it to `"input_raw"`, then wires
/// a `to_doc_item` actor into `"input_doc"`.
fn add_raw_then_doc(
    env: &Arc<Environment>,
    async_runtime: &tokio::runtime::Handle,
    data: Vec<u8>,
) -> Result<(), String> {
    let env_clone = Arc::clone(env);
    let raw_handle = async_runtime
        .block_on(async move { env_clone.add_value_node(data, None).await })
        .map_err(|e| format!("failed to add raw value node: {e}"))?;
    let _h = env.add_alias("input_raw".to_string(), raw_handle);
    wire_to_doc_item(env, raw_handle, &[]);
    Ok(())
}

/// Creates a `file_value` actor node (with path or `"-"` for stdin in `explain`),
/// adds it to `"input_raw"`, then wires a `to_doc_item` actor into `"input_doc"`.
fn add_file_then_doc(
    env: &Arc<Environment>,
    async_runtime: &tokio::runtime::Handle,
    file_explain: String,
    attrs: &[(String, String)],
) {
    let file_handle = env.add_node("file_value".to_string(), &[], Some(file_explain.clone()));
    file_value_control::register(
        file_handle,
        file_explain.clone(),
        attrs.to_vec(),
        Arc::clone(&env.kv),
        Arc::clone(&env.idgen),
        async_runtime.clone(),
    );
    let _h = env.add_alias("input_raw".to_string(), file_handle);
    // For extension-detected images, inject type + content_type so to_doc_item
    // knows to emit an image item rather than text.
    let augmented;
    let doc_attrs = if !attrs.iter().any(|(k, _)| k == "type") {
        if let Some(mime) = file_value::mime_for_path(&file_explain) {
            augmented = {
                let mut v = attrs.to_vec();
                v.push(("type".to_string(), "image".to_string()));
                v.push(("content_type".to_string(), mime.to_string()));
                v
            };
            augmented.as_slice()
        } else {
            attrs
        }
    } else {
        attrs
    };
    wire_to_doc_item(env, file_handle, doc_attrs);
}

/// Creates a `to_doc_item` actor node that depends on `raw_handle`, registers
/// `attrs` in the control registry, and adds the node to the `"input_doc"` alias.
fn wire_to_doc_item(env: &Arc<Environment>, raw_handle: Handle, attrs: &[(String, String)]) {
    let explain = attrs_to_explain(attrs);
    let doc_handle = env.add_node("to_doc_item".to_string(), &[raw_handle], explain);
    to_doc_item_control::register(doc_handle, attrs.to_vec());
    let _h = env.add_alias("input_doc".to_string(), doc_handle);
}

/// Serialises `attrs` as `key=value` pairs joined by `\n` for the `explain` field.
fn attrs_to_explain(attrs: &[(String, String)]) -> Option<String> {
    if attrs.is_empty() {
        return None;
    }
    Some(
        attrs
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Creates `"input_raw"` / `"input_doc"` aliases for each prompt item.
///
/// For each content item the pipeline is:
/// - A raw value node (plain bytes) aliased as `"input_raw"`.
/// - A `to_doc_item` actor node (which will convert raw → structured) aliased
///   as `"input_doc"`.
///
/// CTL role-marker nodes (ctl/user, ctl/system) are structured from the start
/// and are aliased directly as `"input_doc"` — they bypass `"input_raw"`.
///
/// File and stdin items use a `file_value` actor instead of an inline value
/// node; the actor node is aliased as `"input_raw"`.
///
/// Returns `true` if stdin was consumed (any `PromptArg::Stdin` was present).
///
/// # Errors
/// Returns an error if node creation fails.
pub fn register_prompt_inputs(
    env: &Arc<Environment>,
    async_runtime: &tokio::runtime::Handle,
    items: &[PromptArg],
) -> Result<bool, String> {
    let mut last_role: Option<&str> = None;
    let mut stdin_consumed = false;

    for item in items {
        match item {
            PromptArg::SystemPrompt(text) => {
                if last_role != Some("system") {
                    add_ctl_to_input_doc(env, async_runtime, CTL_SYSTEM_JSON)?;
                }
                add_raw_then_doc(env, async_runtime, text.as_bytes().to_vec())?;
                last_role = Some("system");
            }
            PromptArg::Text(text) => {
                if last_role != Some("user") {
                    add_ctl_to_input_doc(env, async_runtime, CTL_USER_JSON)?;
                }
                add_raw_then_doc(env, async_runtime, text.as_bytes().to_vec())?;
                last_role = Some("user");
            }
            PromptArg::Stdin => {
                if last_role != Some("user") {
                    add_ctl_to_input_doc(env, async_runtime, CTL_USER_JSON)?;
                }
                add_file_then_doc(env, async_runtime, "-".to_string(), &[]);
                stdin_consumed = true;
                last_role = Some("user");
            }
            PromptArg::File { path, attrs } => {
                if last_role != Some("user") {
                    add_ctl_to_input_doc(env, async_runtime, CTL_USER_JSON)?;
                }
                add_file_then_doc(env, async_runtime, path.clone(), attrs);
                last_role = Some("user");
            }
        }
    }
    Ok(stdin_consumed)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ailetos::pipe::pipe_path;
    use ailetos::{KVBuffers, MemKV, NodeKind, OpenMode};

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

        assert!(consumed, "stdin should be marked consumed");

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

    // test 10b: explicit stdin at end ends up last
    #[test]
    fn test_explicit_stdin_appended_last() {
        let (env, rt) = make_env();
        let items = vec![
            PromptArg::Text("Hello".to_string()),
            PromptArg::Stdin,
        ];
        let consumed = register_prompt_inputs(&env, rt.handle(), &items).unwrap();

        assert!(consumed);

        let doc_deps = alias_deps(&env, "input_doc");
        assert_eq!(doc_deps.len(), 3);
        assert_eq!(read_value_node(&env, &rt, doc_deps[0]), r#"[{"type":"ctl"},{"role":"user"}]"#);
        assert_eq!(node_idname(&env, doc_deps[1]), "to_doc_item");
        assert_eq!(node_idname(&env, doc_deps[2]), "to_doc_item");

        let raw_deps = alias_deps(&env, "input_raw");
        assert_eq!(raw_deps.len(), 2);
        assert_eq!(read_value_node(&env, &rt, raw_deps[0]), "Hello");
        assert_eq!(node_idname(&env, raw_deps[1]), "file_value");
        assert_eq!(node_explain(&env, raw_deps[1]).as_deref(), Some("-"));
    }

    // test 11: file path stored in file_value explain; attrs forwarded to to_doc_item
    #[test]
    fn test_file_path_in_explain() {
        let (env, rt) = make_env();
        let items = vec![PromptArg::File {
            path: "/some/photo.png".to_string(),
            attrs: vec![
                ("type".to_string(), "image".to_string()),
                ("content_type".to_string(), "image/png".to_string()),
            ],
        }];
        register_prompt_inputs(&env, rt.handle(), &items).unwrap();

        let raw_deps = alias_deps(&env, "input_raw");
        assert_eq!(raw_deps.len(), 1);
        assert_eq!(node_idname(&env, raw_deps[0]), "file_value");
        assert_eq!(
            node_explain(&env, raw_deps[0]).as_deref(),
            Some("/some/photo.png")
        );

        let doc_deps = alias_deps(&env, "input_doc");
        let to_doc = doc_deps.iter().copied().find(|&h| node_idname(&env, h) == "to_doc_item")
            .expect("to_doc_item not found in input_doc");
        let explain = node_explain(&env, to_doc).expect("to_doc_item should have explain");
        assert!(explain.contains("type=image"), "explain={explain}");
        assert!(explain.contains("content_type=image/png"), "explain={explain}");
    }

    // test 12: file with no attrs → to_doc_item has no explain
    #[test]
    fn test_file_no_attrs_no_explain() {
        let (env, rt) = make_env();
        let items = vec![PromptArg::File {
            path: "note.txt".to_string(),
            attrs: vec![],
        }];
        register_prompt_inputs(&env, rt.handle(), &items).unwrap();

        let doc_deps = alias_deps(&env, "input_doc");
        let to_doc = doc_deps.iter().copied().find(|&h| node_idname(&env, h) == "to_doc_item")
            .expect("to_doc_item not found in input_doc");
        assert_eq!(node_explain(&env, to_doc), None);
    }

}
