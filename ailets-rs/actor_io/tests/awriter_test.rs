use actor_io::{error_kind_to_str, AWriter};
use actor_runtime_mocked::{VfsActorRuntime, WANT_ERROR};
use embedded_io::Write;

#[test]
fn happy_path() {
    let runtime = VfsActorRuntime::new();
    let mut writer = AWriter::new(&runtime, "test").expect("Should create writer");

    writer.write_all(b"Hello,").unwrap();
    writer.write_all(b" world!").unwrap();

    assert_eq!(runtime.get_file("test").unwrap(), b"Hello, world!");
}

#[test]
fn write_in_chunks() {
    let runtime = VfsActorRuntime::new();
    let mut writer = AWriter::new(&runtime, "test").expect("Should create writer");

    let data = b"one\ntwo\nthree\n";
    for _ in 0..3 {
        let n = writer.write(data).unwrap();
        assert!(n == 4, "Should write 4 bytes (o, n, e, nl)");
    }
    writer.write_all(data).unwrap();

    assert_eq!(
        runtime.get_file("test").unwrap(),
        b"one\none\none\none\ntwo\nthree\n"
    );
}

#[test]
fn cant_open_nonexistent_file() {
    let runtime = VfsActorRuntime::new();
    let err =
        AWriter::new(&runtime, "file-name-to-fail\u{1}").expect_err("Should fail to create writer");

    assert_eq!(
        err,
        embedded_io::ErrorKind::InvalidInput,
        "Error should be InvalidInput, got: {}",
        error_kind_to_str(err)
    );
}

#[test]
fn close_can_raise_error() {
    let runtime = VfsActorRuntime::new();
    let mut writer = AWriter::new(&runtime, "fname-close-error").expect("Should create writer");

    runtime.clear_mocks();
    writer.close().expect_err("Should fail to close");
}

#[test]
fn write_error() {
    let runtime = VfsActorRuntime::new();
    let mut writer = AWriter::new(&runtime, "fname-write-error").expect("Should create writer");
    let err = writer
        .write(&[WANT_ERROR as u8])
        .expect_err("Should fail to write");

    assert_eq!(
        err,
        embedded_io::ErrorKind::Other,
        "Error should be Other, got: {}",
        error_kind_to_str(err)
    );
}
