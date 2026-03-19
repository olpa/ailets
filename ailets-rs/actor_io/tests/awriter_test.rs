use actor_io::{error_kind_to_str, AWriter};
use actor_runtime::StdHandle;
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

#[test]
fn new_writer_closes_on_drop() {
    let runtime = VfsActorRuntime::new();

    // Create a writer using new() - this should close on drop
    {
        let _writer = AWriter::new(&runtime, "test-close").expect("Should create writer");
        assert_eq!(runtime.close_call_count(), 0, "No close calls yet");
    }

    // After drop, the fd should have been closed
    assert_eq!(
        runtime.close_call_count(),
        1,
        "Writer created with new() should close on drop"
    );
}

#[test]
fn new_from_std_does_not_close_on_drop() {
    let runtime = VfsActorRuntime::new();

    // Create a writer using new_from_std() - this should NOT close on drop
    {
        let _writer = AWriter::new_from_std(&runtime, StdHandle::Stdout);
        assert_eq!(runtime.close_call_count(), 0, "No close calls yet");
    }

    // After drop, no close should have been called
    assert_eq!(
        runtime.close_call_count(),
        0,
        "Writer created with new_from_std() should NOT close on drop"
    );
}

#[test]
fn new_from_fd_closes_on_drop() {
    let runtime = VfsActorRuntime::new();

    // First create a file to get a valid fd
    let fd = runtime.get_file("nonexistent").err(); // This won't work, we need open_write
    drop(fd);

    // Create a writer using new_from_fd() - this should close on drop
    {
        let _writer = AWriter::new_from_fd(&runtime, 42).expect("Should create writer from fd");
        assert_eq!(runtime.close_call_count(), 0, "No close calls yet");
    }

    // After drop, the fd should have been closed
    assert_eq!(
        runtime.close_call_count(),
        1,
        "Writer created with new_from_fd() should close on drop"
    );
    assert!(runtime.was_closed(42), "fd 42 should have been closed");
}
