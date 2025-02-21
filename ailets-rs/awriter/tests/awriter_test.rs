use actor_runtime_mocked::clear_mocks;
use awriter::AWriter;

#[test]
fn cant_open_nonexistent_file() {
    clear_mocks();

    let err = AWriter::new(c"file-name-to-fail\u{1}").expect_err("Should fail to create writer");

    assert!(
        err.to_string().contains("file-name-to-fail\u{1}"),
        "Error message should contain the file name"
    );
}

#[test]
fn close_can_raise_error() {
    clear_mocks();

    let mut writer = AWriter::new(c"fname-close-error").expect("Should create writer");

    clear_mocks();
    writer.close().expect_err("Should fail to close");
}
