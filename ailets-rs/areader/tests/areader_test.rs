use actor_runtime_mocked::{add_file, clear_mocks};
use areader::AReader;
use std::io::Read;

#[test]
fn happy_path() {
    clear_mocks();

    add_file("test.0".to_string(), b"foo".to_vec());
    add_file("test.1".to_string(), b"bar".to_vec());
    add_file("test.2".to_string(), b"baz".to_vec());

    let mut reader = AReader::new(c"test").expect("Should create reader");
    let mut result = String::new();

    reader
        .read_to_string(&mut result)
        .expect("Should read all content");

    assert_eq!(result, "foobarbaz");
}

#[test]
fn read_in_chunks() {
    clear_mocks();

    add_file(
        "chunks.0".to_string(),
        b"first\nchunk\nthird\nfourth\nfifth".to_vec(),
    );
    add_file("chunks.1".to_string(), b"next\nfile\ncontents".to_vec());

    let mut reader = AReader::new(c"chunks").expect("Should create reader");
    let mut buf = [0u8; 10];

    // Read first chunk manually
    let n = reader.read(&mut buf).expect("Should read first chunk");
    assert_eq!(&buf[..n], b"first\n");

    // Read second chunk manually
    let n = reader.read(&mut buf).expect("Should read second chunk");
    assert_eq!(&buf[..n], b"chunk\n");

    // Read the rest
    let mut result = String::new();
    reader
        .read_to_string(&mut result)
        .expect("Should read remaining content");

    assert_eq!(result, "third\nfourth\nfifthnext\nfile\ncontents");
}

#[test]
fn cant_open_nonexistent_file() {
    clear_mocks();

    let err = AReader::new(c"no-such-file").expect_err("Should fail to create reader");

    assert!(
        err.to_string().contains("no-such-file"),
        "Error message should contain the file name"
    );
}

#[test]
fn close_can_raise_error() {
    clear_mocks();

    add_file("fname-close-error.0".to_string(), b"foo".to_vec());

    let mut reader = AReader::new(c"fname-close-error").expect("Should create reader");

    // Mock a close error by clearing the mocks while file is still open
    clear_mocks();
    let err = reader.close().expect_err("Should fail to close");
    assert!(
        err.to_string().contains("fname-close-error"),
        "Error message should contain the stream name"
    );
}

#[test]
fn read_error() {
    clear_mocks();

    add_file(
        "fname-read-error.0".to_string(),
        vec![actor_runtime_mocked::WANT_ERROR as u8],
    );

    let mut reader = AReader::new(c"fname-read-error").expect("Should create reader");
    let mut buf = [0u8; 10];

    let err = reader.read(&mut buf).expect_err("Should fail to read");
    assert!(
        err.to_string().contains("fname-read-error"),
        "Error message should contain the stream name"
    );
}
