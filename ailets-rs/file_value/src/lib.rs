//! Actor: reads a file or stdin and writes raw bytes (or a KV key for images) to stdout.
//!
//! Path is read from `/var/{pid}/path`. Attrs are read from the remaining
//! `/var/{pid}/...` entries. KV writing for images uses `runtime.open_write`.
//!
//! Behaviour by content type:
//!   text / stdin  → raw bytes written directly to stdout
//!   image         → bytes stored in KV under `media/{pid}`; the key written to stdout
//!
//! Type is determined first by the `type` attr, then by file extension.

use actor_io::AWriter;
use actor_runtime::var_access::{list_var_keys, read_var};
use actor_runtime::{ActorRuntime, StdHandle};
use embedded_io::Write as _;

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

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

/// # Errors
/// Returns an error if I/O fails, the path var is missing, or the file type is unknown.
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let path = read_var(runtime, "path")?
        .ok_or_else(|| "file_value: 'path' var not set".to_string())?;

    let attrs: Vec<(String, String)> = list_var_keys(runtime)
        .into_iter()
        .filter(|k| k != "path")
        .filter_map(|k| {
            let v = read_var(runtime, &k).ok()??;
            Some((k, v))
        })
        .collect();

    let mut writer = AWriter::new_from_std(runtime, StdHandle::Stdout);

    let content_kind = detect_kind(&path, &attrs)?;

    match content_kind {
        ContentKind::Stdin | ContentKind::Text => {
            let mut src = open_source(&path)?;
            std::io::copy(&mut src, &mut writer)
                .map_err(|e| format!("file_value: copy error: {e}"))?;
        }
        ContentKind::Image => {
            let filename = std::path::Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("image");
            let image_key = format!("/input/{}/{filename}", uuid::Uuid::new_v4());
            let mut kv_writer = AWriter::new(runtime, &image_key)
                .map_err(|e| format!("file_value: kv open failed: {e:?}"))?;
            let mut src = open_source(&path)?;
            std::io::copy(&mut src, &mut kv_writer)
                .map_err(|e| format!("file_value: kv copy error: {e}"))?;
            drop(kv_writer);
            writer
                .write_all(image_key.as_bytes())
                .map_err(|e| format!("file_value: write error: {e:?}"))?;
        }
    }

    Ok(())
}

#[derive(Debug)]
pub enum ContentKind {
    Stdin,
    Text,
    Image,
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

fn extension_of(path: &str) -> &str {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
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
            other => Err(format!("file_value: unknown type attr '{other}'")),
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
        "file_value: unknown file type{hint} for '{path}'; \
         use @type=text,file=... or @type=image,content_type=...,file=..."
    ))
}

fn open_source(path: &str) -> Result<Box<dyn std::io::Read>, String> {
    if path == "-" {
        Ok(Box::new(std::io::stdin()))
    } else {
        let f = std::fs::File::open(path)
            .map_err(|e| format!("file_value: failed to open '{path}': {e}"))?;
        Ok(Box::new(f))
    }
}
