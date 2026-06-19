use to_doc_item::build_frame;

fn assemble(raw: &[u8], attrs: &[(String, String)]) -> Result<Vec<u8>, String> {
    let (prefix, suffix) = build_frame(attrs)?;
    let mut out = prefix;
    out.extend_from_slice(raw);
    out.extend_from_slice(suffix);
    Ok(out)
}

#[test]
fn text_no_attrs() {
    let out = assemble(b"hello world", &[]).unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":"hello world"}]"#);
}

#[test]
fn text_explicit_type_attr() {
    let attrs = vec![("type".to_string(), "text".to_string())];
    let out = assemble(b"hi", &attrs).unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":"hi"}]"#);
}

#[test]
fn empty_input() {
    let out = assemble(b"", &[]).unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":""}]"#);
}

#[test]
fn image_with_content_type() {
    let attrs = vec![
        ("type".to_string(), "image".to_string()),
        ("content_type".to_string(), "image/png".to_string()),
    ];
    let out = assemble(b"media/42", &attrs).unwrap();
    assert_eq!(
        out,
        br#"[{"type":"image","content_type":"image/png"},{"image_key":"media/42"}]"#
    );
}

#[test]
fn image_missing_content_type_errors() {
    let attrs = vec![("type".to_string(), "image".to_string())];
    assert!(build_frame(&attrs).is_err());
}

#[test]
fn unknown_type_errors() {
    let attrs = vec![("type".to_string(), "video".to_string())];
    assert!(build_frame(&attrs).is_err());
}
