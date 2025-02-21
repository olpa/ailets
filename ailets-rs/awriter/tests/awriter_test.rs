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
