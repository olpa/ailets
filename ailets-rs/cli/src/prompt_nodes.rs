//! Build DAG nodes and aliases from parsed prompt items, and session dispatch.

use std::sync::Arc;

use ailetos::{Environment, Handle};

use crate::shell_ui::PromptArg;
use file_value::control as file_value_control;
use to_doc_item::control as to_doc_item_control;

const CTL_USER_JSON: &[u8] = br#"[{"type":"ctl"},{"role":"user"}]"#;
const CTL_SYSTEM_JSON: &[u8] = br#"[{"type":"ctl"},{"role":"system"}]"#;

/// How stdin is used after `register_prompt_inputs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinUsage {
    /// Stdin is wired into a DAG `file_value` actor node.
    FileValue,
    /// Stdin is available for the interactive shell.
    DagShell,
}

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
    file_explain: &str,
    attrs: &[(String, String)],
) {
    let file_handle = env.add_node("file_value".to_string(), &[], Some(file_explain.to_string()));
    file_value_control::register(
        file_handle,
        file_explain.to_string(),
        attrs.to_vec(),
        Arc::clone(&env.kv),
        Arc::clone(&env.idgen),
        async_runtime.clone(),
    );
    let _h = env.add_alias("input_raw".to_string(), file_handle);
    // For extension-detected images, inject type + content_type so to_doc_item
    // knows to emit an image item rather than text.
    let augmented;
    let doc_attrs = if attrs.iter().any(|(k, _)| k == "type") {
        attrs
    } else if let Some(mime) = file_value::mime_for_path(file_explain) {
        augmented = {
            let mut v = attrs.to_vec();
            v.push(("type".to_string(), "image".to_string()));
            v.push(("content_type".to_string(), mime.to_string()));
            v
        };
        augmented.as_slice()
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
/// Returns `StdinUsage::FileValue` if any `PromptArg::Stdin` was present.
///
/// # Errors
/// Returns an error if node creation fails.
pub fn register_prompt_inputs(
    env: &Arc<Environment>,
    async_runtime: &tokio::runtime::Handle,
    items: &[PromptArg],
) -> Result<StdinUsage, String> {
    let mut last_role: Option<&str> = None;
    let mut stdin_usage = StdinUsage::DagShell;

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
                add_file_then_doc(env, async_runtime, "-", &[]);
                stdin_usage = StdinUsage::FileValue;
                last_role = Some("user");
            }
            PromptArg::File { path, attrs } => {
                if last_role != Some("user") {
                    add_ctl_to_input_doc(env, async_runtime, CTL_USER_JSON)?;
                }
                add_file_then_doc(env, async_runtime, path, attrs);
                last_role = Some("user");
            }
        }
    }
    Ok(stdin_usage)
}
