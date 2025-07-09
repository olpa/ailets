use actor_io::AWriter;
use actor_runtime_mocked::{clear_mocks, get_file, WANT_ERROR};
use std::io::Write;

#[test]
fn happy_path() {
    clear_mocks();
    let mut writer = AWriter::new(c"test").expect("Should create writer");

    writer.write_all(b"Hello,").unwrap();
    writer.write_all(b" world!").unwrap();

    assert_eq!(get_file("test").unwrap(), b"Hello, world!");
}

#[test]
fn write_in_chunks() {
    clear_mocks();
    let mut writer = AWriter::new(c"test").expect("Should create writer");

    let data = b"one\ntwo\nthree\n";
    for _ in 0..3 {
        let n = writer.write(data).unwrap();
        assert!(n == 4, "Should write 4 bytes (o, n, e, nl)");
    }
    writer.write_all(data).unwrap();

    assert_eq!(
        get_file("test").unwrap(),
        b"one\none\none\none\ntwo\nthree\n"
    );
}

#[test]
fn cant_open_nonexistent_file() {
    clear_mocks();

    let err = AWriter::new(c"file-name-to-fail\u{1}").expect_err("Should fail to create writer");

    assert!(
        err.to_string().contains("os error"),
        "Error message should contain os error"
    );
}

#[test]
fn close_can_raise_error() {
    clear_mocks();

    let mut writer = AWriter::new(c"fname-close-error").expect("Should create writer");

    clear_mocks();
    writer.close().expect_err("Should fail to close");
}

#[test]
fn write_error() {
    clear_mocks();

    let mut writer = AWriter::new(c"fname-write-error").expect("Should create writer");
    let err = writer
        .write(&[WANT_ERROR as u8])
        .expect_err("Should fail to write");

    assert!(
        err.to_string().contains("os error"),
        "Error message should indicate write failure"
    );
}

#[test]
fn write_to_value_node() {
    use actor_io::AWriter;
    use actor_runtime_mocked::{clear_mocks, dag_value_node, get_file};
    use std::ffi::CString;
    use std::io::Write;

    clear_mocks();

    // Create a value node named "foo"
    let value = CString::new("foo").unwrap();
    let explain = CString::new("test value node").unwrap();
    let handle = dag_value_node(value.as_ptr().cast::<u8>(), explain.as_ptr());
    assert!(handle >= 0);

    // Write to the value node
    let mut writer =
        AWriter::new_for_value_node(handle).expect("Should create writer for value node");
    writer.write_all(b"hello value node!").unwrap();
    writer.close().unwrap();

    // Check the file content
    let written = get_file("foo").unwrap();
    assert_eq!(written, b"hello value node!");
}
