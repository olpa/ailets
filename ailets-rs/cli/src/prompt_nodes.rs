//! Build DAG nodes and aliases from parsed prompt items, and session dispatch.

use std::sync::Arc;

use ailetos::Environment;

use crate::shell_ui::PromptArg;

const CTL_USER_JSON: &[u8] = br#"[{"type":"ctl"},{"role":"user"}]"#;
const CTL_SYSTEM_JSON: &[u8] = br#"[{"type":"ctl"},{"role":"system"}]"#;

fn add_value_alias(
    env: &Arc<Environment>,
    rt: &tokio::runtime::Handle,
    data: Vec<u8>,
) -> Result<(), String> {
    let env_clone = Arc::clone(env);
    let handle = rt
        .block_on(async move { env_clone.add_value_node(data, None).await })
        .map_err(|e| format!("failed to add value node: {e}"))?;
    env.add_alias("input".to_string(), handle);
    Ok(())
}

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
    env: &Arc<Environment>,
    rt: &tokio::runtime::Handle,
    items: &[PromptArg],
    stdin_handle: Option<ailetos::Handle>,
) -> Result<bool, String> {
    let mut user_ctl_inserted = false;
    let mut stdin_consumed = false;

    for item in items {
        match item {
            PromptArg::SystemPrompt(text) => {
                add_value_alias(env, rt, CTL_SYSTEM_JSON.to_vec())?;
                let json = format!(r#"[{{"type":"text"}},{{"text":"{text}"}}]"#);
                add_value_alias(env, rt, json.into_bytes())?;
            }
            PromptArg::Text(text) => {
                if !user_ctl_inserted {
                    add_value_alias(env, rt, CTL_USER_JSON.to_vec())?;
                    user_ctl_inserted = true;
                }
                let json = format!(r#"[{{"type":"text"}},{{"text":"{text}"}}]"#);
                add_value_alias(env, rt, json.into_bytes())?;
            }
            PromptArg::Stdin => {
                if !user_ctl_inserted {
                    add_value_alias(env, rt, CTL_USER_JSON.to_vec())?;
                    user_ctl_inserted = true;
                }
                let handle = stdin_handle
                    .ok_or_else(|| "Stdin item present but no stdin_handle provided".to_string())?;
                env.add_alias("input".to_string(), handle);
                stdin_consumed = true;
            }
            PromptArg::File { path, attrs } => {
                if !user_ctl_inserted {
                    add_value_alias(env, rt, CTL_USER_JSON.to_vec())?;
                    user_ctl_inserted = true;
                }
                let json = file_to_content_item(path, attrs, env, rt)?;
                add_value_alias(env, rt, json.into_bytes())?;
            }
        }
    }
    Ok(stdin_consumed)
}

const TEXT_EXTENSIONS: &[&str] = &["txt", "md", "rs", "py", "js", "ts", "json", "toml", "yaml", "yml", "html", "css", "sh"];
const IMAGE_EXTENSIONS: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
];

fn extension_of(path: &str) -> Option<&str> {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
}

/// Reads a file and returns a `ContentItem` JSON string for use in a value node.
///
/// For image files, stores the raw bytes in KV and embeds the key in the JSON.
///
/// # Errors
/// Returns an error for unknown file extensions (without explicit attrs).
pub fn file_to_content_item(
    path: &str,
    _attrs: &[(String, String)],
    env: &Arc<Environment>,
    rt: &tokio::runtime::Handle,
) -> Result<String, String> {
    let ext = extension_of(path)
        .ok_or_else(|| format!("cannot determine file type: no extension on '{path}'"))?
        .to_lowercase();

    if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read '{path}': {e}"))?;
        // escape backslash and double-quote for JSON
        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
        return Ok(format!(r#"[{{"type":"text"}},{{"text":"{escaped}"}}]"#));
    }

    if let Some(&(_, content_type)) = IMAGE_EXTENSIONS.iter().find(|(e, _)| *e == ext.as_str()) {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("failed to read '{path}': {e}"))?;
        let image_key = format!("media/{}", env.idgen.get_next());
        let key_clone = image_key.clone();
        let kv = Arc::clone(&env.kv);
        rt.block_on(async move {
            use ailetos::{OpenMode, KVBuffers};
            let buf = kv.open(&key_clone, OpenMode::Write).await
                .map_err(|e| format!("kv write failed: {e}"))?;
            buf.append(&bytes)
                .map_err(|e| format!("kv append failed: {e}"))?;
            Ok::<(), String>(())
        })?;
        return Ok(format!(
            r#"[{{"type":"image","content_type":"{content_type}"}},{{"image_key":"{image_key}"}}]"#
        ));
    }

    Err(format!("unknown file extension '.{ext}' for '{path}'; cannot determine content type"))
}

// ---------------------------------------------------------------------------
// Session dispatch
// ---------------------------------------------------------------------------

/// What the shell should do after processing prompt items.
#[derive(Debug, PartialEq)]
pub enum SessionMode {
    /// No prompt items: open interactive REPL (existing behaviour).
    Interactive,
    /// No prompt items + `-l` script: run script then keep REPL open.
    LoadThenInteractive,
    /// Prompt items created, stdin not consumed: open REPL (nodes already wired).
    PromptThenInteractive,
    /// Prompt items + `-l` script (stdin not consumed): run script then exit.
    PromptLoadThenExit,
    /// Prompt items + stdin consumed (no `-l`): exit after nodes are created.
    PromptThenExit,
    /// Prompt items + stdin consumed + `-l` script: run script then exit.
    PromptLoadStdinThenExit,
}

/// Decide the post-prompt session behaviour from the three boolean inputs.
/// Stub — always returns `Interactive`.
#[must_use]
pub fn decide_session(
    _has_prompt_items: bool,
    _stdin_consumed: bool,
    _has_load_script: bool,
) -> SessionMode {
    SessionMode::Interactive
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

    // test 11: text extension (.txt, .md) → [{"type":"text"},{"text":"..."}]
    #[test]
    fn test_file_text_extension() {
        let (env, rt) = make_env();
        let dir = tempfile::tempdir().unwrap();

        let txt_path = dir.path().join("note.txt");
        std::fs::write(&txt_path, "hello world").unwrap();
        let json =
            file_to_content_item(txt_path.to_str().unwrap(), &[], &env, rt.handle()).unwrap();
        assert_eq!(json, r#"[{"type":"text"},{"text":"hello world"}]"#);

        let md_path = dir.path().join("readme.md");
        std::fs::write(&md_path, "# Title").unwrap();
        let json =
            file_to_content_item(md_path.to_str().unwrap(), &[], &env, rt.handle()).unwrap();
        assert_eq!(json, r##"[{"type":"text"},{"text":"# Title"}]"##);
    }

    // test 12: image extension (.png, .jpg) →
    //   [{"type":"image","content_type":"image/png"},{"image_key":"<key>"}]
    //   and raw bytes stored in KV at that key
    #[test]
    fn test_file_image_extension() {
        let (env, rt) = make_env();
        let dir = tempfile::tempdir().unwrap();

        let png_bytes: &[u8] = b"\x89PNG\r\n\x1a\n";
        let png_path = dir.path().join("photo.png");
        std::fs::write(&png_path, png_bytes).unwrap();

        let json =
            file_to_content_item(png_path.to_str().unwrap(), &[], &env, rt.handle()).unwrap();

        // JSON must contain type:image and content_type
        assert!(json.contains(r#""type":"image""#), "json={json}");
        assert!(json.contains(r#""content_type":"image/png""#), "json={json}");

        // Extract image_key and verify bytes are stored in KV
        let image_key = {
            let start = json.find(r#""image_key":""#).expect("image_key not found")
                + r#""image_key":""#.len();
            let end = json[start..].find('"').unwrap() + start;
            json[start..end].to_string()
        };
        assert!(!image_key.is_empty());

        let stored = rt.block_on(async {
            let kv = Arc::clone(&env.kv);
            let buf = kv.open(&image_key, OpenMode::Read).await.unwrap();
            let data = buf.lock().to_vec();
            data
        });
        assert_eq!(stored, png_bytes);
    }

    // test 13: unknown extension without attrs → descriptive error
    #[test]
    fn test_file_unknown_extension_error() {
        let (env, rt) = make_env();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.bin");
        std::fs::write(&path, b"some bytes").unwrap();

        let result = file_to_content_item(path.to_str().unwrap(), &[], &env, rt.handle());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains(".bin") || msg.contains("unknown") || msg.contains("extension"),
            "error should mention extension or 'unknown': {msg}"
        );
    }

    // Session dispatch — 6 rows from the spec table

    #[test]
    fn test_session_no_items_no_script() {
        assert_eq!(decide_session(false, false, false), SessionMode::Interactive);
    }

    #[test]
    fn test_session_no_items_with_script() {
        assert_eq!(
            decide_session(false, false, true),
            SessionMode::LoadThenInteractive
        );
    }

    #[test]
    fn test_session_items_no_stdin_no_script() {
        assert_eq!(
            decide_session(true, false, false),
            SessionMode::PromptThenInteractive
        );
    }

    #[test]
    fn test_session_items_no_stdin_with_script() {
        assert_eq!(
            decide_session(true, false, true),
            SessionMode::PromptLoadThenExit
        );
    }

    #[test]
    fn test_session_items_stdin_no_script() {
        assert_eq!(
            decide_session(true, true, false),
            SessionMode::PromptThenExit
        );
    }

    #[test]
    fn test_session_items_stdin_with_script() {
        assert_eq!(
            decide_session(true, true, true),
            SessionMode::PromptLoadStdinThenExit
        );
    }
}
