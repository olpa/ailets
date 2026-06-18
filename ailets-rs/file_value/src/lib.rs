//! Actor: reads a file or stdin and writes raw bytes (or a KV key for images) to stdout.
//!
//! Configuration (path, attrs, KV, IdGen) is retrieved from `control`
//! using the actor's node handle.
//!
//! Behaviour by content type:
//!   text / stdin  → raw bytes written directly to stdout
//!   image         → bytes stored in KV under `media/<id>`; the key written to stdout
//!
//! Type is determined first by the `type` attr, then by file extension.

mod actor_registry;
pub mod control;

use actor_io::AWriter;
use actor_runtime::{ActorRuntime, StdHandle};
use ailetos::Handle;
use embedded_io::Write as _;
use std::io::Read as _;

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
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
/// Returns an error if the actor is not registered, I/O fails, or the file type is unknown.
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let my_handle = Handle::new(runtime.node_handle());
    let cfg = control::take(my_handle)
        .ok_or_else(|| format!("file_value actor {my_handle:?} not registered"))?;

    let mut writer = AWriter::new_from_std(runtime, StdHandle::Stdout);

    let content_kind = detect_kind(&cfg.path, &cfg.attrs)?;

    match content_kind {
        ContentKind::Stdin | ContentKind::Text => {
            let raw = read_source(&cfg.path)?;
            writer
                .write_all(&raw)
                .map_err(|e| format!("file_value: write error: {e:?}"))?;
        }
        ContentKind::Image => {
            let raw = read_source(&cfg.path)?;
            let image_key = format!("media/{}", cfg.idgen.get_next());
            let kv = cfg.kv;
            let image_key_for_task = image_key.clone();
            let (tx, rx) = tokio::sync::oneshot::channel::<Result<(), String>>();
            cfg.async_runtime.spawn(async move {
                use ailetos::OpenMode;
                let result = async {
                    let buf = kv
                        .open(&image_key_for_task, OpenMode::Write)
                        .await
                        .map_err(|e| format!("file_value: kv open failed: {e}"))?;
                    buf.append(&raw)
                        .map_err(|e| format!("file_value: kv append failed: {e}"))?;
                    kv.flush_buffer(&buf)
                        .await
                        .map_err(|e| format!("file_value: kv flush failed: {e}"))?;
                    Ok::<(), String>(())
                }
                .await;
                // receiver dropped only if the blocking thread panicked
                if tx.send(result).is_err() {
                    unreachable!("file_value: kv task result receiver dropped");
                }
            });
            rx.blocking_recv()
                .map_err(|_| "file_value: kv task dropped before completing".to_string())??;
            writer
                .write_all(image_key.as_bytes())
                .map_err(|e| format!("file_value: write error: {e:?}"))?;
        }
    }

    Ok(())
}

#[derive(Debug)]
enum ContentKind {
    Stdin,
    Text,
    Image,
}

/// Returns the MIME type for a path whose extension matches a known image
/// extension, or `None` if the extension is not recognised.
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

fn detect_kind(path: &str, attrs: &[(String, String)]) -> Result<ContentKind, String> {
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

fn read_source(path: &str) -> Result<Vec<u8>, String> {
    if path == "-" {
        let mut buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buf)
            .map_err(|e| format!("file_value: stdin read error: {e}"))?;
        Ok(buf)
    } else {
        std::fs::read(path).map_err(|e| format!("file_value: failed to read '{path}': {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_for_known_extensions() {
        assert_eq!(mime_for_path("photo.png"), Some("image/png"));
        assert_eq!(mime_for_path("photo.jpg"), Some("image/jpeg"));
        assert_eq!(mime_for_path("photo.jpeg"), Some("image/jpeg"));
        assert_eq!(mime_for_path("photo.gif"), Some("image/gif"));
        assert_eq!(mime_for_path("photo.webp"), Some("image/webp"));
        assert_eq!(mime_for_path("PHOTO.PNG"), Some("image/png"));
    }

    #[test]
    fn mime_for_non_image_returns_none() {
        assert_eq!(mime_for_path("readme.txt"), None);
        assert_eq!(mime_for_path("data.bin"), None);
        assert_eq!(mime_for_path("-"), None);
    }

    #[test]
    fn detect_stdin() {
        assert!(matches!(detect_kind("-", &[]).unwrap(), ContentKind::Stdin));
    }

    #[test]
    fn detect_text_by_extension() {
        assert!(matches!(detect_kind("note.txt", &[]).unwrap(), ContentKind::Text));
        assert!(matches!(detect_kind("readme.md", &[]).unwrap(), ContentKind::Text));
        assert!(matches!(detect_kind("src.rs", &[]).unwrap(), ContentKind::Text));
    }

    #[test]
    fn detect_image_by_extension() {
        assert!(matches!(detect_kind("photo.png", &[]).unwrap(), ContentKind::Image));
        assert!(matches!(detect_kind("pic.jpg", &[]).unwrap(), ContentKind::Image));
    }

    #[test]
    fn detect_text_by_attr() {
        let attrs = vec![("type".to_string(), "text".to_string())];
        assert!(matches!(detect_kind("data.bin", &attrs).unwrap(), ContentKind::Text));
    }

    #[test]
    fn detect_image_by_attr() {
        let attrs = vec![("type".to_string(), "image".to_string())];
        assert!(matches!(detect_kind("data.bin", &attrs).unwrap(), ContentKind::Image));
    }

    #[test]
    fn unknown_extension_errors() {
        let result = detect_kind("data.bin", &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(".bin"));
    }

    #[test]
    fn unknown_type_attr_errors() {
        let attrs = vec![("type".to_string(), "video".to_string())];
        assert!(detect_kind("file.mp4", &attrs).is_err());
    }

    #[test]
    fn attr_overrides_extension() {
        let attrs = vec![("type".to_string(), "text".to_string())];
        assert!(matches!(detect_kind("data.png", &attrs).unwrap(), ContentKind::Text));
    }
}
