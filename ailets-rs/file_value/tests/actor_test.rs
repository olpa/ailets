use file_value::{detect_kind, mime_for_path, ContentKind};

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
