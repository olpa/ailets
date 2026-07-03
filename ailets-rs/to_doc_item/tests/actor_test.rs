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
