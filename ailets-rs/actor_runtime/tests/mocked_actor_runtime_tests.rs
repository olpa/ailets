use actor_runtime::mocked_actor_runtime::{
    aclose, add_file, clear_mocks, get_file, n_of_streams, open_read, open_write, WANT_ERROR,
};
use std::ffi::CString;

#[test]
fn n_of_streams_returns_zero() {
    clear_mocks();

    let name = CString::new("test").unwrap();
    let n = n_of_streams(name.as_ptr());

    assert_eq!(n, 0);
}
#[test]
fn n_of_streams_returns_number_of_sequential_files() {
    clear_mocks();

    let name = CString::new("foo").unwrap();
    let name_str = name.to_str().unwrap();
    for i in [0, 1, 2, 10] {
        add_file(format!("{name_str}.{i}"), Vec::new());
    }

    let n = n_of_streams(name.as_ptr());

    // Should only count sequential files (0,1,2) and not 10
    assert_eq!(n, 3);
}

#[test]
fn open_read_returns_minus_one_if_file_not_found() {
    clear_mocks();

    let name = CString::new("test").unwrap();
    let fd = open_read(name.as_ptr(), 0);

    assert_eq!(fd, -1);
}

#[test]
fn open_read_returns_non_negative_if_file_exists() {
    clear_mocks();

    let name = CString::new("test").unwrap();
    add_file("test.0".to_string(), Vec::new());
    let fd = open_read(name.as_ptr(), 0);

    assert!(fd >= 0);
}

#[test]
fn open_write_returns_minus_one_on_error() {
    clear_mocks();

    let name = CString::new(format!("test{}", WANT_ERROR)).unwrap();
    let fd = open_write(name.as_ptr());

    assert_eq!(fd, -1);
}

#[test]
fn open_write_creates_file() {
    clear_mocks();

    let name = CString::new("test").unwrap();
    let name_str = name.to_str().unwrap();

    // File should not exist before
    assert!(get_file(name_str).is_err());

    let fd = open_write(name.as_ptr());
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
    let name_foo = CString::new("foo").unwrap();
    let read_fd = open_read(name_foo.as_ptr(), 0);
    assert!(read_fd >= 0);
    let name_bar = CString::new("bar").unwrap();
    let write_fd = open_write(name_bar.as_ptr());
    assert!(write_fd >= 0);

    // Act and assert: Close handles
    let result = aclose(read_fd);
    assert_eq!(result, 0);
    let result = aclose(write_fd);
    assert_eq!(result, 0);
}
