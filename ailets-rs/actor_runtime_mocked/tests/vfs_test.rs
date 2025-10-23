use actor_runtime_mocked::{Vfs, WANT_ERROR};
use std::os::raw::c_uint;

#[test]
fn open_read_returns_minus_one_if_file_not_found() {
    let vfs = Vfs::new();
    let fd = vfs.open_read(c"test".as_ptr());
    assert_eq!(fd, -1);
    assert_eq!(vfs.get_errno(), 2); // ENOENT - No such file or directory
}

#[test]
fn open_read_returns_non_negative_if_file_exists() {
    let vfs = Vfs::new();

    vfs.add_file("test".to_string(), Vec::new());
    let fd = vfs.open_read(c"test".as_ptr());

    assert!(fd >= 0);
    assert_eq!(vfs.get_errno(), 0);
}

#[test]
fn open_write_returns_minus_one_on_error() {
    let vfs = Vfs::new();

    let fd = vfs.open_write(c"test\u{1}".as_ptr());

    assert_eq!(fd, -1);
    assert_eq!(vfs.get_errno(), 22); // EINVAL - Invalid argument
}

#[test]
fn open_write_creates_file() {
    let vfs = Vfs::new();

    let name = c"test";
    let name_str = name.to_str().unwrap();

    // File should not exist before
    assert!(vfs.get_file(name_str).is_err());

    let fd = vfs.open_write(name.as_ptr());
    assert!(fd >= 0);

    // File should exist after open_write
    assert!(vfs.get_file(name_str).is_ok());
}

#[test]
fn close_returns_minus_one_for_invalid_handle() {
    let vfs = Vfs::new();

    let result = vfs.aclose(999);

    assert_eq!(result, -1);
    assert_eq!(vfs.get_errno(), 9); // EBADF - Bad file descriptor
}

#[test]
fn close_returns_zero_if_ok_for_read_and_write_handles() {
    let vfs = Vfs::new();
    vfs.add_file("foo".to_string(), Vec::new());

    // Open handles
    let read_fd = vfs.open_read(c"foo".as_ptr());
    assert!(read_fd >= 0);
    let write_fd = vfs.open_write(c"bar".as_ptr());
    assert!(write_fd >= 0);

    // Act and assert: Close handles
    let result = vfs.aclose(read_fd);
    assert_eq!(result, 0);
    let result = vfs.aclose(write_fd);
    assert_eq!(result, 0);
    assert_eq!(vfs.get_errno(), 0);
}

#[test]
fn read_returns_minus_one_for_invalid_handle() {
    let vfs = Vfs::new();

    let mut buffer = [0u8; 10];
    let result = vfs.aread(999, buffer.as_mut_ptr(), buffer.len() as c_uint);

    assert_eq!(result, -1);
    assert_eq!(vfs.get_errno(), 9); // EBADF - Bad file descriptor
}

#[test]
fn read_returns_all_content() {
    let vfs = Vfs::new();

    // Create test file
    let content = b"Hello World!";
    vfs.add_file("test".to_string(), content.to_vec());

    // Open file for reading
    let fd = vfs.open_read(c"test".as_ptr());
    assert!(fd >= 0);

    // Read entire content
    let mut buffer = [0u8; 32];
    let bytes_read = vfs.aread(fd, buffer.as_mut_ptr(), buffer.len() as c_uint);

    assert_eq!(bytes_read, content.len() as i32);
    assert_eq!(&buffer[..content.len()], content);

    // Verify EOF (should return 0 bytes)
    let bytes_read = vfs.aread(fd, buffer.as_mut_ptr(), buffer.len() as c_uint);
    assert_eq!(bytes_read, 0);
    assert_eq!(vfs.get_errno(), 0);
}

#[test]
fn read_in_chunks_with_io_interrupt() {
    let vfs = Vfs::new();

    // Create test file with IO_INTERRUPT character
    let file_content = format!("one\ntwo\nthree\nx{WANT_ERROR}x");
    vfs.add_file("test".to_string(), file_content.as_bytes().to_vec());

    // Open file for reading
    let fd = vfs.open_read(c"test".as_ptr());
    assert!(fd >= 0);

    // Read first chunk
    let mut buffer = [0u8; 10];
    let bytes_read = vfs.aread(fd, buffer.as_mut_ptr(), buffer.len() as c_uint);
    assert_eq!(bytes_read, 4);
    assert_eq!(&buffer[..4], b"one\n");

    // Read second chunk
    let bytes_read = vfs.aread(fd, buffer.as_mut_ptr(), buffer.len() as c_uint);
    assert_eq!(bytes_read, 4);
    assert_eq!(&buffer[..4], b"two\n");

    // Read third chunk
    let bytes_read = vfs.aread(fd, buffer.as_mut_ptr(), buffer.len() as c_uint);
    assert_eq!(bytes_read, 6);
    assert_eq!(&buffer[..6], b"three\n");

    // Get an error
    let bytes_read = vfs.aread(fd, buffer.as_mut_ptr(), buffer.len() as c_uint);
    assert_eq!(bytes_read, -1);
    assert_eq!(vfs.get_errno(), 5); // EIO - I/O error
}

#[test]
fn write_returns_minus_one_for_invalid_handle() {
    let vfs = Vfs::new();

    let buffer = [1u8, 2, 3];
    let bytes_written = vfs.awrite(999, buffer.as_ptr() as *mut u8, buffer.len() as c_uint);
    assert_eq!(bytes_written, -1);
    assert_eq!(vfs.get_errno(), 9); // EBADF - Bad file descriptor
}

#[test]
fn write_returns_bytes_written() {
    let vfs = Vfs::new();

    // Open file for writing
    let fd = vfs.open_write(c"test".as_ptr());
    assert!(fd >= 0);

    // Write some content
    let content = b"Hello world!";
    let bytes_written = vfs.awrite(fd, content.as_ptr() as *mut u8, content.len() as c_uint);

    assert_eq!(bytes_written, content.len() as i32);

    // Verify written content
    let written = vfs.get_file("test").unwrap();
    assert_eq!(written, content);
    assert_eq!(vfs.get_errno(), 0);
}

#[test]
fn write_all_content() {
    let vfs = Vfs::new();

    let fd = vfs.open_write(c"test".as_ptr());
    assert!(fd >= 0);

    // Write some content
    let content = b"Hello world!";
    let bytes_written = vfs.awrite(fd, content.as_ptr() as *mut u8, content.len() as c_uint);
    assert_eq!(bytes_written, content.len() as i32);

    // Verify written content
    let written = vfs.get_file("test").unwrap();
    assert_eq!(written, content);
    assert_eq!(vfs.get_errno(), 0);
}

#[test]
fn write_in_chunks_with_io_interrupt() {
    let vfs = Vfs::new();
    let content = format!("one\ntwo\nthree\nx{WANT_ERROR}x");
    let content_buffer = content.as_bytes();

    // Open file for writing
    let fd = vfs.open_write(c"test".as_ptr());
    assert!(fd >= 0);

    // Write first chunk
    let bytes_written = vfs.awrite(fd, content_buffer.as_ptr() as *mut u8, 100);
    assert_eq!(bytes_written, 4);
    assert_eq!(vfs.get_errno(), 0);

    // Write second chunk
    let bytes_written = vfs.awrite(
        fd,
        unsafe { content_buffer.as_ptr().add(4) } as *mut u8,
        100,
    );
    assert_eq!(bytes_written, 4);
    assert_eq!(vfs.get_errno(), 0);

    // Write third chunk
    let bytes_written = vfs.awrite(
        fd,
        unsafe { content_buffer.as_ptr().add(8) } as *mut u8,
        100,
    );
    assert_eq!(bytes_written, 6);
    assert_eq!(vfs.get_errno(), 0);
    // Write fourth chunk and get an error
    let bytes_written = vfs.awrite(
        fd,
        unsafe { content_buffer.as_ptr().add(14) } as *mut u8,
        100,
    );
    assert_eq!(bytes_written, -1);
    assert_eq!(vfs.get_errno(), 5); // EIO - I/O error

    // Verify complete (until error) content
    let written = vfs.get_file("test").unwrap();
    assert_eq!(written, b"one\ntwo\nthree\nx");
}
