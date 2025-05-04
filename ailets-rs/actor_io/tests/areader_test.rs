use actor_io::AReader;
use actor_runtime_mocked::{add_file, clear_mocks};
use std::io::Read;

#[test]
fn happy_path() {
    clear_mocks();

    add_file("test".to_string(), b"foo".to_vec());

    let mut reader = AReader::new(c"test").expect("Should create reader");
    let mut result = String::new();

    reader
        .read_to_string(&mut result)
        .expect("Should read all content");

    assert_eq!(result, "foo");
}

#[test]
fn read_in_chunks() {
    clear_mocks();

    add_file(
        "chunks".to_string(),
        b"first\nchunk\nthird\nfourth\nfifth".to_vec(),
    );

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

    assert_eq!(result, "third\nfourth\nfifth");
}

#[test]
fn cant_open_nonexistent_file() {
    clear_mocks();

    let err = AReader::new(c"no-such-file").expect_err("Should fail to create reader");

    assert!(
        err.to_string().contains("os error"),
        "Error message should contain os error"
    );
}

#[test]
fn read_error() {
    clear_mocks();

    add_file(
        "fname-read-error".to_string(),
        vec![actor_runtime_mocked::WANT_ERROR as u8],
    );

    let mut reader = AReader::new(c"fname-read-error").expect("Should create reader");
    let mut buf = [0u8; 10];

    reader.read(&mut buf).expect_err("Should fail to read");
}
