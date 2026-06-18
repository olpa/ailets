use to_doc_item::build_content_item;

#[test]
fn text_no_attrs() {
    let out = build_content_item(b"hello world", &[]).unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":"hello world"}]"#);
}

#[test]
fn text_explicit_type_attr() {
    let attrs = vec![("type".to_string(), "text".to_string())];
    let out = build_content_item(b"hi", &attrs).unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":"hi"}]"#);
}

#[test]
fn empty_input() {
    let out = build_content_item(b"", &[]).unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":""}]"#);
}

#[test]
fn image_with_content_type() {
    let attrs = vec![
        ("type".to_string(), "image".to_string()),
        ("content_type".to_string(), "image/png".to_string()),
    ];
    let out = build_content_item(b"media/42", &attrs).unwrap();
    assert_eq!(
        out,
        br#"[{"type":"image","content_type":"image/png"},{"image_key":"media/42"}]"#
    );
}

#[test]
fn image_missing_content_type_errors() {
    let attrs = vec![("type".to_string(), "image".to_string())];
    assert!(build_content_item(b"key", &attrs).is_err());
}

#[test]
fn unknown_type_errors() {
    let attrs = vec![("type".to_string(), "video".to_string())];
    assert!(build_content_item(b"data", &attrs).is_err());
}
