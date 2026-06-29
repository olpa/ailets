use actor_runtime_mocked::{RcWriter, VfsActorRuntime};
use embedded_io::Write as _;
use to_doc_item::build_frame;

fn make_runtime(attrs: &[(&str, &str)]) -> VfsActorRuntime {
    let runtime = VfsActorRuntime::new();
    for (key, value) in attrs {
        let path = format!("/var/0/{key}");
        runtime.add_file(path, value.as_bytes().to_vec());
    }
    runtime
}

fn assemble(raw: &[u8], runtime: &VfsActorRuntime) -> Result<Vec<u8>, String> {
    let mut writer = RcWriter::new();
    build_frame(runtime, &mut writer)?;
    writer.write_all(raw).map_err(|e| format!("{e:?}"))?;
    writer.write_all(br#""}]"#).map_err(|e| format!("{e:?}"))?;
    Ok(writer.get_output().into_bytes())
}

#[test]
fn text_no_attrs() {
    let runtime = make_runtime(&[]);
    let out = assemble(b"hello world", &runtime).unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":"hello world"}]"#);
}

#[test]
fn text_explicit_type_attr() {
    let runtime = make_runtime(&[("AILETS_DOC_ITEM_type", "text")]);
    let out = assemble(b"hi", &runtime).unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":"hi"}]"#);
}

#[test]
fn empty_input() {
    let runtime = make_runtime(&[]);
    let out = assemble(b"", &runtime).unwrap();
    assert_eq!(out, br#"[{"type":"text"},{"text":""}]"#);
}

#[test]
fn image_with_content_type() {
    let runtime = make_runtime(&[
        ("AILETS_DOC_ITEM_type", "image"),
        ("AILETS_DOC_ITEM_content_type", "image/png"),
    ]);
    let out = assemble(b"media/42", &runtime).unwrap();
    assert_eq!(
        out,
        br#"[{"type":"image","content_type":"image/png"},{"image_key":"media/42"}]"#
    );
}

#[test]
fn unknown_type_errors() {
    let runtime = make_runtime(&[("AILETS_DOC_ITEM_type", "video")]);
    let mut writer = RcWriter::new();
    assert!(build_frame(&runtime, &mut writer).is_err());
}

#[test]
fn text_extra_attrs_included() {
    let runtime = make_runtime(&[
        ("AILETS_DOC_ITEM_type", "text"),
        ("AILETS_DOC_ITEM_lang", "en"),
    ]);
    let out = assemble(b"hi", &runtime).unwrap();
    assert_eq!(out, br#"[{"type":"text","lang":"en"},{"text":"hi"}]"#);
}

#[test]
fn image_extra_attrs_included() {
    let runtime = make_runtime(&[
        ("AILETS_DOC_ITEM_type", "image"),
        ("AILETS_DOC_ITEM_content_type", "image/png"),
        ("AILETS_DOC_ITEM_detail", "high"),
    ]);
    let out = assemble(b"media/42", &runtime).unwrap();
    assert_eq!(
        out,
        br#"[{"type":"image","content_type":"image/png","detail":"high"},{"image_key":"media/42"}]"#
    );
}
