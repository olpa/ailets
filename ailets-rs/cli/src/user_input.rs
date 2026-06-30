//! DAG wiring for user-supplied content: text, files, and stdin.
//!
//! Builds the `input_raw` → `to_doc_item` → `input_doc` pipeline for each
//! prompt item, including MIME-type detection for image files.

use std::sync::Arc;

use ailetos::{Environment, Handle};

use crate::shell_ui::PromptArg;

// ---------------------------------------------------------------------------
// MIME / content-kind helpers
// ---------------------------------------------------------------------------

const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "rs", "py", "js", "ts", "json", "toml", "yaml", "yml", "html", "css", "sh",
];
const IMAGE_EXTENSIONS: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
];

fn extension_of(path: &str) -> &str {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
}

/// Returns the MIME type for a path whose extension matches a known image
/// extension, or `None` if the extension is not recognised.
#[must_use]
pub fn mime_for_path(path: &str) -> Option<&'static str> {
    let ext = extension_of(path).to_lowercase();
    IMAGE_EXTENSIONS
        .iter()
        .find(|(e, _)| *e == ext.as_str())
        .map(|(_, mime)| *mime)
}

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[derive(Debug)]
pub enum ContentKind {
    Stdin,
    Text,
    Image,
}

/// # Errors
/// Returns an error if the `type` attribute has an unrecognised value.
pub fn detect_kind(path: &str, attrs: &[(String, String)]) -> Result<ContentKind, String> {
    if path == "-" {
        return Ok(ContentKind::Stdin);
    }

    if let Some(t) = attr(attrs, "type") {
        return match t {
            "text" => Ok(ContentKind::Text),
            "image" => Ok(ContentKind::Image),
            other => Err(format!("user_input: unknown type attr '{other}'")),
        };
    }

    let ext = extension_of(path).to_lowercase();

    if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        return Ok(ContentKind::Text);
    }
    if IMAGE_EXTENSIONS.iter().any(|(e, _)| *e == ext.as_str()) {
        return Ok(ContentKind::Image);
    }

    let hint = if ext.is_empty() {
        String::new()
    } else {
        format!(" '.{ext}'")
    };
    Err(format!(
        "user_input: unknown file type{hint} for '{path}'; \
         use @type=text,file=... or @type=image,content_type=...,file=..."
    ))
}

// ---------------------------------------------------------------------------
// DAG wiring helpers
// ---------------------------------------------------------------------------

const CTL_USER_JSON: &[u8] = br#"[{"type":"ctl"},{"role":"user"}]"#;
const CTL_SYSTEM_JSON: &[u8] = br#"[{"type":"ctl"},{"role":"system"}]"#;

/// Creates a value node from `data` and adds it to the `"input_doc"` alias.
pub fn add_ctl_to_input_doc(
    env: &Arc<Environment>,
    async_runtime: &tokio::runtime::Handle,
    data: &[u8],
) -> Result<(), String> {
    let env_clone = Arc::clone(env);
    let explain = Some(String::from_utf8_lossy(data).into_owned());
    let data = data.to_vec();
    let handle = async_runtime
        .block_on(async move { env_clone.add_value_node(data, explain).await })
        .map_err(|e| format!("failed to add ctl value node: {e}"))?;
    let _h = env.add_alias("input_doc".to_string(), handle);
    Ok(())
}

pub fn add_ctl_user(
    env: &Arc<Environment>,
    async_runtime: &tokio::runtime::Handle,
) -> Result<(), String> {
    add_ctl_to_input_doc(env, async_runtime, CTL_USER_JSON)
}

pub fn add_ctl_system(
    env: &Arc<Environment>,
    async_runtime: &tokio::runtime::Handle,
) -> Result<(), String> {
    add_ctl_to_input_doc(env, async_runtime, CTL_SYSTEM_JSON)
}

/// Creates a raw value node from `data`, adds it to `"input_raw"`, then wires
/// a `to_doc_item` actor into `"input_doc"`.
pub fn add_raw_then_doc(
    env: &Arc<Environment>,
    async_runtime: &tokio::runtime::Handle,
    data: Vec<u8>,
) -> Result<(), String> {
    let env_clone = Arc::clone(env);
    let explain = text_explain(&data);
    let raw_handle = async_runtime
        .block_on(async move { env_clone.add_value_node(data, explain).await })
        .map_err(|e| format!("failed to add raw value node: {e}"))?;
    let _h = env.add_alias("input_raw".to_string(), raw_handle);
    wire_to_doc_item(env, raw_handle, &[]);
    Ok(())
}

/// Creates a `file_value` actor node, adds it to `"input_raw"`, then wires a
/// `to_doc_item` actor into `"input_doc"`.
///
/// For image files, `content_type` is injected into the `to_doc_item` attrs so
/// it can emit the correct image doc-item frame.
pub fn add_file_then_doc(env: &Arc<Environment>, path: &str, attrs: &[(String, String)]) {
    let file_handle = env.add_node("file_value".to_string(), &[], file_explain(path));
    let pid = file_handle.id();
    env.var_store.set(Some(pid), "path", path);
    let _h = env.add_alias("input_raw".to_string(), file_handle);

    let augmented;
    let doc_attrs = if attrs.iter().any(|(k, _)| k == "type") {
        attrs
    } else if let Some(mime) = mime_for_path(path) {
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

/// Creates a `to_doc_item` actor node that depends on `raw_handle`, sets attrs
/// via var_store, and adds the node to the `"input_doc"` alias.
pub fn wire_to_doc_item(env: &Arc<Environment>, raw_handle: Handle, attrs: &[(String, String)]) {
    let doc_handle = env.add_node("to_doc_item".to_string(), &[raw_handle], None);
    let pid = doc_handle.id();
    for (key, value) in attrs {
        let prefixed_key = format!("AILETS_DOC_ITEM_{key}");
        env.var_store.set(Some(pid), prefixed_key.as_str(), value.as_str());
    }
    let _h = env.add_alias("input_doc".to_string(), doc_handle);
}

fn make_safe(s: &str) -> String {
    s.chars()
        .filter(|&c| c as u32 >= 32)
        .collect::<String>()
        .trim()
        .to_string()
}

fn text_explain(data: &[u8]) -> Option<String> {
    let safe = make_safe(&String::from_utf8_lossy(data));
    if safe.is_empty() {
        return None;
    }
    let chars: Vec<char> = safe.chars().collect();
    if chars.len() <= 20 {
        Some(safe)
    } else {
        Some(chars[..20].iter().collect::<String>() + "...")
    }
}

fn file_explain(path: &str) -> Option<String> {
    if path == "-" {
        return Some("stdin".to_string());
    }
    let name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);
    let safe = make_safe(name);
    if safe.is_empty() {
        None
    } else {
        Some(safe)
    }
}

// ---------------------------------------------------------------------------
// Top-level entry point
// ---------------------------------------------------------------------------

/// How stdin is used after `register_prompt_inputs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinUsage {
    /// Stdin is wired into a DAG `file_value` actor node.
    FileValue,
    /// Stdin is available for the interactive shell.
    DagShell,
}

/// Creates `"input_raw"` / `"input_doc"` aliases for each prompt item.
///
/// # Errors
/// Returns an error if node creation fails or a file type cannot be determined.
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
                    add_ctl_system(env, async_runtime)?;
                }
                add_raw_then_doc(env, async_runtime, text.as_bytes().to_vec())?;
                last_role = Some("system");
            }
            PromptArg::Text(text) => {
                if last_role != Some("user") {
                    add_ctl_user(env, async_runtime)?;
                }
                add_raw_then_doc(env, async_runtime, text.as_bytes().to_vec())?;
                last_role = Some("user");
            }
            PromptArg::Stdin => {
                if last_role != Some("user") {
                    add_ctl_user(env, async_runtime)?;
                }
                add_file_then_doc(env, "-", &[]);
                stdin_usage = StdinUsage::FileValue;
                last_role = Some("user");
            }
            PromptArg::File { path, attrs } => {
                if last_role != Some("user") {
                    add_ctl_user(env, async_runtime)?;
                }
                add_file_then_doc(env, path, attrs);
                last_role = Some("user");
            }
        }
    }
    Ok(stdin_usage)
}
