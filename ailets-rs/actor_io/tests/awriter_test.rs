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

#[test]
fn write_error() {
    clear_mocks();

    let mut writer = AWriter::new(c"fname-write-error").expect("Should create writer");
    let err = writer
        .write(&[WANT_ERROR as u8])
        .expect_err("Should fail to write");

    assert!(
        err.to_string().contains("Failed to write"),
        "Error message should indicate write failure"
    );
}
