use actor_runtime_mocked::{Vfs, WANT_ERROR};

#[test]
fn open_read_returns_error_if_file_not_found() {
    let vfs = Vfs::new();
    let result = vfs.open_read("test");
    assert_eq!(result, Err(2)); // ENOENT - No such file or directory
}

#[test]
fn open_read_returns_fd_if_file_exists() {
    let vfs = Vfs::new();

    vfs.add_file("test".to_string(), Vec::new());
    let fd = vfs.open_read("test").unwrap();

    assert!(fd >= 0);
}

#[test]
fn open_write_returns_error_on_want_error() {
    let vfs = Vfs::new();

    let result = vfs.open_write("test\u{1}");

    assert_eq!(result, Err(22)); // EINVAL - Invalid argument
}

#[test]
fn open_write_creates_file() {
    let vfs = Vfs::new();

    let name = "test";

    // File should not exist before
    assert!(vfs.get_file(name).is_err());

    let fd = vfs.open_write(name).unwrap();
    assert!(fd >= 0);

    // File should exist after open_write
    assert!(vfs.get_file(name).is_ok());
}

#[test]
fn close_returns_error_for_invalid_handle() {
    let vfs = Vfs::new();

    let result = vfs.aclose(999);

    assert_eq!(result, Err(9)); // EBADF - Bad file descriptor
}

#[test]
fn close_returns_ok_for_read_and_write_handles() {
    let vfs = Vfs::new();
    vfs.add_file("foo".to_string(), Vec::new());

    // Open handles
    let read_fd = vfs.open_read("foo").unwrap();
    let write_fd = vfs.open_write("bar").unwrap();

    // Act and assert: Close handles
    assert!(vfs.aclose(read_fd).is_ok());
    assert!(vfs.aclose(write_fd).is_ok());
}

#[test]
fn read_returns_error_for_invalid_handle() {
    let vfs = Vfs::new();

    let mut buffer = [0u8; 10];
    let result = vfs.aread(999, &mut buffer);

    assert_eq!(result, Err(9)); // EBADF - Bad file descriptor
}

#[test]
fn read_returns_all_content() {
    let vfs = Vfs::new();

    // Create test file
    let content = b"Hello World!";
    vfs.add_file("test".to_string(), content.to_vec());

    // Open file for reading
    let fd = vfs.open_read("test").unwrap();

    // Read entire content
    let mut buffer = [0u8; 32];
    let bytes_read = vfs.aread(fd, &mut buffer).unwrap();

    assert_eq!(bytes_read, content.len());
    assert_eq!(&buffer[..content.len()], content);

    // Verify EOF (should return 0 bytes)
    let bytes_read = vfs.aread(fd, &mut buffer).unwrap();
    assert_eq!(bytes_read, 0);
}

#[test]
fn read_in_chunks_with_io_interrupt() {
    let vfs = Vfs::new();

    // Create test file with IO_INTERRUPT character
    let file_content = format!("one\ntwo\nthree\nx{WANT_ERROR}x");
    vfs.add_file("test".to_string(), file_content.as_bytes().to_vec());

    // Open file for reading
    let fd = vfs.open_read("test").unwrap();

    // Read first chunk
    let mut buffer = [0u8; 10];
    let bytes_read = vfs.aread(fd, &mut buffer).unwrap();
    assert_eq!(bytes_read, 4);
    assert_eq!(&buffer[..4], b"one\n");

    // Read second chunk
    let bytes_read = vfs.aread(fd, &mut buffer).unwrap();
    assert_eq!(bytes_read, 4);
    assert_eq!(&buffer[..4], b"two\n");

    // Read third chunk
    let bytes_read = vfs.aread(fd, &mut buffer).unwrap();
    assert_eq!(bytes_read, 6);
    assert_eq!(&buffer[..6], b"three\n");

    // Get an error
    let result = vfs.aread(fd, &mut buffer);
    assert_eq!(result, Err(5)); // EIO - I/O error
}

#[test]
fn write_returns_error_for_invalid_handle() {
    let vfs = Vfs::new();

    let buffer = [1u8, 2, 3];
    let result = vfs.awrite(999, &buffer);
    assert_eq!(result, Err(9)); // EBADF - Bad file descriptor
}

#[test]
fn write_returns_bytes_written() {
    let vfs = Vfs::new();

    // Open file for writing
    let fd = vfs.open_write("test").unwrap();

    // Write some content
    let content = b"Hello world!";
    let bytes_written = vfs.awrite(fd, content).unwrap();

    assert_eq!(bytes_written, content.len());

    // Verify written content
    let written = vfs.get_file("test").unwrap();
    assert_eq!(written, content);
}

#[test]
fn write_all_content() {
    let vfs = Vfs::new();

    let fd = vfs.open_write("test").unwrap();

    // Write some content
    let content = b"Hello world!";
    let bytes_written = vfs.awrite(fd, content).unwrap();
    assert_eq!(bytes_written, content.len());

    // Verify written content
    let written = vfs.get_file("test").unwrap();
    assert_eq!(written, content);
}

#[test]
fn write_in_chunks_with_io_interrupt() {
    let vfs = Vfs::new();
    let content = format!("one\ntwo\nthree\nx{WANT_ERROR}x");
    let content_buffer = content.as_bytes();

    // Open file for writing
    let fd = vfs.open_write("test").unwrap();

    // Write first chunk
    let bytes_written = vfs.awrite(fd, &content_buffer[0..]).unwrap();
    assert_eq!(bytes_written, 4);

    // Write second chunk
    let bytes_written = vfs.awrite(fd, &content_buffer[4..]).unwrap();
    assert_eq!(bytes_written, 4);

    // Write third chunk
    let bytes_written = vfs.awrite(fd, &content_buffer[8..]).unwrap();
    assert_eq!(bytes_written, 6);

    // Write fourth chunk and get an error
    let result = vfs.awrite(fd, &content_buffer[14..]);
    assert_eq!(result, Err(5)); // EIO - I/O error

    // Verify complete (until error) content
    let written = vfs.get_file("test").unwrap();
    assert_eq!(written, b"one\ntwo\nthree\nx");
}
