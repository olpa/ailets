use actor_runtime::mocked_actor_runtime::{
    aclose, add_file, aread, awrite, clear_mocks, get_file, n_of_streams, open_read, open_write,
    WANT_ERROR,
};

#[test]
fn n_of_streams_returns_zero() {
    clear_mocks();
    let n = n_of_streams(c"test".as_ptr() as *const u8);
    assert_eq!(n, 0);
}

#[test]
fn n_of_streams_returns_number_of_sequential_files() {
    clear_mocks();
    let name = c"foo";
    let name_str = name.to_str().unwrap();
    for i in [0, 1, 2, 10] {
        add_file(format!("{name_str}.{i}"), Vec::new());
    }

    let n = n_of_streams(name.as_ptr() as *const u8);

    // Should only count sequential files (0,1,2) and not 10
    assert_eq!(n, 3);
}

#[test]
fn open_read_returns_minus_one_if_file_not_found() {
    clear_mocks();
    let fd = open_read(c"test".as_ptr() as *const u8, 0);
    assert_eq!(fd, -1);
}

#[test]
fn open_read_returns_non_negative_if_file_exists() {
    clear_mocks();

    add_file("test.0".to_string(), Vec::new());
    let fd = open_read(c"test".as_ptr() as *const u8, 0);

    assert!(fd >= 0);
}

#[test]
fn open_write_returns_minus_one_on_error() {
    clear_mocks();

    let fd = open_write(c"test\u{1}".as_ptr() as *const u8);

    assert_eq!(fd, -1);
}

#[test]
fn open_write_creates_file() {
    clear_mocks();

    let name = c"test";
    let name_str = name.to_str().unwrap();

    // File should not exist before
    assert!(get_file(name_str).is_err());

    let fd = open_write(name.as_ptr() as *const u8);
    assert!(fd >= 0);

    // File should exist after open_write
    assert!(get_file(name_str).is_ok());
}

#[test]
fn close_returns_minus_one_for_invalid_handle() {
    clear_mocks();

    let result = aclose(999);

    assert_eq!(result, -1);
}

#[test]
fn close_returns_zero_if_ok_for_read_and_write_handles() {
    clear_mocks();
    add_file("foo.0".to_string(), Vec::new());

    // Open handles
    let read_fd = open_read(c"foo".as_ptr() as *const u8, 0);
    assert!(read_fd >= 0);
    let write_fd = open_write(c"bar".as_ptr() as *const u8);
    assert!(write_fd >= 0);

    // Act and assert: Close handles
    let result = aclose(read_fd);
    assert_eq!(result, 0);
    let result = aclose(write_fd);
    assert_eq!(result, 0);
}

#[test]
fn read_returns_minus_one_for_invalid_handle() {
    clear_mocks();

    let mut buffer = [0u8; 10];
    let result = aread(999, buffer.as_mut_ptr(), buffer.len());

    assert_eq!(result, -1);
}

#[test]
fn read_returns_all_content() {
    clear_mocks();

    // Create test file
    let content = b"Hello World!";
    add_file("test.0".to_string(), content.to_vec());

    // Open file for reading
    let fd = open_read(c"test".as_ptr() as *const u8, 0);
    assert!(fd >= 0);

    // Read entire content
    let mut buffer = [0u8; 32];
    let bytes_read = aread(fd, buffer.as_mut_ptr(), buffer.len());

    assert_eq!(bytes_read, content.len() as i32);
    assert_eq!(&buffer[..content.len()], content);

    // Verify EOF (should return 0 bytes)
    let bytes_read = aread(fd, buffer.as_mut_ptr(), buffer.len());
    assert_eq!(bytes_read, 0);
}

#[test]
fn read_in_chunks_with_io_interrupt() {
    clear_mocks();

    // Create test file with IO_INTERRUPT character
    let file_content = format!("one\ntwo\nthree\nx{WANT_ERROR}x");
    add_file("test.0".to_string(), file_content.as_bytes().to_vec());

    // Open file for reading
    let fd = open_read(c"test".as_ptr() as *const u8, 0);
    assert!(fd >= 0);

    // Read first chunk
    let mut buffer = [0u8; 10];
    let bytes_read = aread(fd, buffer.as_mut_ptr(), buffer.len());
    assert_eq!(bytes_read, 4);
    assert_eq!(&buffer[..4], b"one\n");

    // Read second chunk
    let bytes_read = aread(fd, buffer.as_mut_ptr(), buffer.len());
    assert_eq!(bytes_read, 4);
    assert_eq!(&buffer[..4], b"two\n");

    // Read third chunk
    let bytes_read = aread(fd, buffer.as_mut_ptr(), buffer.len());
    assert_eq!(bytes_read, 6);
    assert_eq!(&buffer[..6], b"three\n");

    // Get an error
    let bytes_read = aread(fd, buffer.as_mut_ptr(), buffer.len());
    assert_eq!(bytes_read, -1);
}

#[test]
fn write_returns_minus_one_for_invalid_handle() {
    clear_mocks();

    let buffer = [1u8, 2, 3];
    let bytes_written = awrite(999, buffer.as_ptr() as *mut u8, buffer.len());
    assert_eq!(bytes_written, -1);
}

#[test]
fn write_returns_bytes_written() {
    clear_mocks();

    // Open file for writing
    let fd = open_write(c"test".as_ptr() as *const u8);
    assert!(fd >= 0);

    // Write some content
    let content = b"Hello world!";
    let bytes_written = awrite(fd, content.as_ptr() as *mut u8, content.len());

    assert_eq!(bytes_written, content.len() as i32);

    // Verify written content
    let written = get_file("test").unwrap();
    assert_eq!(written, content);
}

#[test]
fn write_all_content() {
    clear_mocks();

    let fd = open_write(c"test".as_ptr() as *const u8);
    assert!(fd >= 0);

    // Write some content
    let content = b"Hello world!";
    let bytes_written = awrite(fd, content.as_ptr() as *mut u8, content.len());
    assert_eq!(bytes_written, content.len() as i32);

    // Verify written content
    let written = get_file("test").unwrap();
    assert_eq!(written, content);
}

#[test]
fn write_in_chunks_with_io_interrupt() {
    clear_mocks();
    let content = format!("one\ntwo\nthree\nx{WANT_ERROR}x");
    let content_buffer = content.as_bytes();

    // Open file for writing
    let fd = open_write(c"test".as_ptr() as *const u8);
    assert!(fd >= 0);

    // Write first chunk
    let bytes_written = awrite(fd, content_buffer.as_ptr() as *mut u8, 100);
    assert_eq!(bytes_written, 4);

    // Write second chunk
    let bytes_written = awrite(
        fd,
        unsafe { content_buffer.as_ptr().add(4) } as *mut u8,
        100,
    );
    assert_eq!(bytes_written, 4);

    // Write third chunk
    let bytes_written = awrite(
        fd,
        unsafe { content_buffer.as_ptr().add(8) } as *mut u8,
        100,
    );
    assert_eq!(bytes_written, 6);

    // Write fourth chunk and get an error
    let bytes_written = awrite(
        fd,
        unsafe { content_buffer.as_ptr().add(14) } as *mut u8,
        100,
    );
    assert_eq!(bytes_written, -1);

    // Verify complete (until error) content
    let written = get_file("test").unwrap();
    assert_eq!(written, b"one\ntwo\nthree\nx");
}
