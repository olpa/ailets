use actor_runtime::mocked_actor_runtime::{add_file, clear_mocks, n_of_streams, open_read};
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
fn open_read_returns_negative_one_if_no_file() {
    clear_mocks();

    let name = CString::new("test").unwrap();
    let fd = open_read(name.as_ptr(), 0);

    assert_eq!(fd, -1);
}
