use actor_runtime::mocked_actor_runtime::{clear_mocks, open_read};
use std::ffi::CString;

#[test]
fn open_read_returns_negative_one_if_no_file() {
    clear_mocks();

    let name = CString::new("test").unwrap();
    let fd = open_read(name.as_ptr(), 0);

    assert_eq!(fd, -1);
}
