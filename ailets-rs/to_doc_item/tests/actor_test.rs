use actor_runtime_mocked::VfsActorRuntime;
use to_doc_item::execute_impl;

fn run(attrs: &[(&str, &str)], input: &[u8]) -> Result<Vec<u8>, String> {
    let runtime = VfsActorRuntime::new();
    for (key, value) in attrs {
        runtime.add_file(format!("/var/0/{key}"), value.as_bytes().to_vec());
    }
    let mut output = Vec::new();
    execute_impl(&runtime, input, &mut output)?;
    Ok(output)
}

#[test]
fn text_no_attrs() {
    let out = run(&[], b"hello world").unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":"hello world"}]"#);
}

#[test]
fn text_explicit_type_attr() {
    let out = run(&[("AILETS_DOC_ITEM_type", "text")], b"hi").unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":"hi"}]"#);
}

#[test]
fn empty_input() {
    let out = run(&[], b"").unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":""}]"#);
}

#[test]
fn image_with_content_type() {
    let out = run(
        &[
            ("AILETS_DOC_ITEM_type", "image"),
            ("AILETS_DOC_ITEM_content_type", "image/png"),
        ],
        b"media/42",
    )
    .unwrap();
    assert_eq!(
        out,
        br#"[{"type":"image","content_type":"image/png"},{"image_key":"media/42"}]"#
    );
}

#[test]
fn unknown_type_errors() {
    assert!(run(&[("AILETS_DOC_ITEM_type", "video")], b"").is_err());
}

#[test]
fn text_extra_attrs_included() {
    let out = run(
        &[
            ("AILETS_DOC_ITEM_type", "text"),
            ("AILETS_DOC_ITEM_lang", "en"),
        ],
        b"hi",
    )
    .unwrap();
    assert_eq!(out, br#"[{"type":"text","lang":"en"},{"text":"hi"}]"#);
}

#[test]
fn image_extra_attrs_included() {
    let out = run(
        &[
            ("AILETS_DOC_ITEM_type", "image"),
            ("AILETS_DOC_ITEM_content_type", "image/png"),
            ("AILETS_DOC_ITEM_detail", "high"),
        ],
        b"media/42",
    )
    .unwrap();
    assert_eq!(
        out,
        br#"[{"type":"image","content_type":"image/png","detail":"high"},{"image_key":"media/42"}]"#
    );
}
