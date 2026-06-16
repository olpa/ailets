//! Build DAG nodes and aliases from parsed prompt items.

use std::sync::Arc;

use ailetos::Environment;

use crate::shell_ui::PromptArg;

/// Creates value nodes and `input` aliases for each prompt item.
///
/// A ctl(user) node is auto-inserted once immediately before the first
/// non-`SystemPrompt` item. Stdin items reference the existing `shell_input`
/// actor node whose handle is passed in `stdin_handle`.
///
/// Returns `true` if stdin was consumed (any `PromptArg::Stdin` was present).
///
/// # Errors
/// Returns an error if a `File` item cannot be read or its type is unknown.
pub fn register_prompt_inputs(
    _env: &Arc<Environment>,
    _rt: &tokio::runtime::Handle,
    _items: &[PromptArg],
    _stdin_handle: Option<ailetos::Handle>,
) -> Result<bool, String> {
    Err("not yet implemented".to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ailetos::pipe::pipe_path;
    use ailetos::{Handle, KVBuffers, MemKV, NodeKind, OpenMode};

    fn make_env() -> (Arc<Environment>, tokio::runtime::Runtime) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let kv = Arc::new(MemKV::new());
        let env = Arc::new(Environment::new(Arc::clone(&kv) as Arc<dyn KVBuffers>));
        (env, rt)
    }

    fn input_alias_deps(env: &Arc<Environment>) -> Vec<Handle> {
        let dag = env.dag.read();
        let alias = dag
            .nodes()
            .find(|n| n.kind == NodeKind::Alias && n.idname == "input")
            .expect("input alias not found");
        dag.get_direct_dependencies(alias.pid).collect()
    }

    fn read_node_content(
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

    // test 8: ctl(user) auto-inserted once before first non-SystemPrompt item;
    //         subsequent items do not trigger a second ctl(user)
    #[test]
    fn test_ctl_user_inserted_once() {
        let (env, rt) = make_env();
        let items = vec![
            PromptArg::Text("Hello".to_string()),
            PromptArg::Text("World".to_string()),
        ];
        register_prompt_inputs(&env, rt.handle(), &items, None).unwrap();

        let deps = input_alias_deps(&env);
        assert_eq!(deps.len(), 3, "expected ctl(user) + 2 text nodes");

        assert_eq!(
            read_node_content(&env, &rt, deps[0]),
            r#"[{"type":"ctl"},{"role":"user"}]"#
        );
        assert_eq!(
            read_node_content(&env, &rt, deps[1]),
            r#"[{"type":"text"},{"text":"Hello"}]"#
        );
        assert_eq!(
            read_node_content(&env, &rt, deps[2]),
            r#"[{"type":"text"},{"text":"World"}]"#
        );
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
        register_prompt_inputs(&env, rt.handle(), &items, None).unwrap();

        let deps = input_alias_deps(&env);
        assert_eq!(deps.len(), 5, "expected ctl(user) + text + ctl(system) + text + text");

        assert_eq!(
            read_node_content(&env, &rt, deps[0]),
            r#"[{"type":"ctl"},{"role":"user"}]"#
        );
        assert_eq!(
            read_node_content(&env, &rt, deps[1]),
            r#"[{"type":"text"},{"text":"Hello"}]"#
        );
        assert_eq!(
            read_node_content(&env, &rt, deps[2]),
            r#"[{"type":"ctl"},{"role":"system"}]"#
        );
        assert_eq!(
            read_node_content(&env, &rt, deps[3]),
            r#"[{"type":"text"},{"text":"Be formal"}]"#
        );
        assert_eq!(
            read_node_content(&env, &rt, deps[4]),
            r#"[{"type":"text"},{"text":"World"}]"#
        );
    }

    // test 10a: explicit Stdin stays at its position in the sequence
    #[test]
    fn test_explicit_stdin_stays_at_position() {
        let (env, rt) = make_env();
        let stdin_node = env.add_node("shell_input".to_string(), &[], None);

        let items = vec![
            PromptArg::Stdin,
            PromptArg::Text("Hello".to_string()),
        ];
        let consumed =
            register_prompt_inputs(&env, rt.handle(), &items, Some(stdin_node)).unwrap();

        assert!(consumed, "stdin should be marked consumed");
        let deps = input_alias_deps(&env);
        assert_eq!(deps.len(), 3, "expected ctl(user) + stdin + text");
        assert_eq!(deps[0], stdin_node, "ctl(user) should come first... wait");

        // ctl(user) is first, then stdin at its position, then text
        assert_eq!(
            read_node_content(&env, &rt, deps[0]),
            r#"[{"type":"ctl"},{"role":"user"}]"#
        );
        assert_eq!(deps[1], stdin_node);
        assert_eq!(
            read_node_content(&env, &rt, deps[2]),
            r#"[{"type":"text"},{"text":"Hello"}]"#
        );
    }

    // test 10b: implicit stdin (appended by TTY check) ends up last
    #[test]
    fn test_implicit_stdin_appended_last() {
        let (env, rt) = make_env();
        let stdin_node = env.add_node("shell_input".to_string(), &[], None);

        // TTY check in main appends Stdin before calling register_prompt_inputs
        let items = vec![
            PromptArg::Text("Hello".to_string()),
            PromptArg::Stdin,
        ];
        let consumed =
            register_prompt_inputs(&env, rt.handle(), &items, Some(stdin_node)).unwrap();

        assert!(consumed);
        let deps = input_alias_deps(&env);
        assert_eq!(deps.len(), 3, "expected ctl(user) + text + stdin");
        assert_eq!(
            read_node_content(&env, &rt, deps[0]),
            r#"[{"type":"ctl"},{"role":"user"}]"#
        );
        assert_eq!(
            read_node_content(&env, &rt, deps[1]),
            r#"[{"type":"text"},{"text":"Hello"}]"#
        );
        assert_eq!(deps[2], stdin_node);
    }
}
