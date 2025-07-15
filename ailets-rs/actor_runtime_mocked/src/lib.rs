pub mod rc_writer;
pub mod vfs;
pub mod vfs_writer;

use lazy_static::lazy_static;
use std::os::raw::{c_char, c_int, c_uint};

pub use rc_writer::RcWriter;
pub use vfs::{Vfs, IO_INTERRUPT, WANT_ERROR};
pub use vfs_writer::VfsWriter;

lazy_static! {
    static ref VFS_INSTANCE: Vfs = Vfs::new();
}

pub fn clear_mocks() {
    VFS_INSTANCE.clear_mocks();
}

pub fn add_file(name: String, buffer: Vec<u8>) {
    VFS_INSTANCE.add_file(name, buffer);
}

/// Gets the content of a file from the virtual file system.
///
/// # Errors
///
/// Returns an error if the file is not found in the virtual file system,
/// or if there's an issue acquiring the internal mutex lock.
pub fn get_file(name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error + '_>> {
    VFS_INSTANCE.get_file(name)
}

#[no_mangle]
pub extern "C" fn open_read(name_ptr: *const c_char) -> c_int {
    VFS_INSTANCE.open_read(name_ptr)
}

#[no_mangle]
pub extern "C" fn open_write(name_ptr: *const c_char) -> c_int {
    VFS_INSTANCE.open_write(name_ptr)
}

#[no_mangle]
pub extern "C" fn aread(fd: c_int, buffer_ptr: *mut u8, count: c_uint) -> c_int {
    VFS_INSTANCE.aread(fd, buffer_ptr, count)
}

#[no_mangle]
pub extern "C" fn awrite(fd: c_int, buffer_ptr: *mut u8, count: c_uint) -> c_int {
    VFS_INSTANCE.awrite(fd, buffer_ptr, count)
}

#[no_mangle]
pub extern "C" fn aclose(fd: c_int) -> c_int {
    VFS_INSTANCE.aclose(fd)
}

#[no_mangle]
pub extern "C" fn get_errno() -> c_int {
    VFS_INSTANCE.get_errno()
}

#[no_mangle]
pub extern "C" fn dag_value_node(value_ptr: *const u8, explain_ptr: *const c_char) -> c_int {
    VFS_INSTANCE.dag_value_node(value_ptr, explain_ptr)
}

#[no_mangle]
pub extern "C" fn open_write_value_node(node_handle: c_int) -> c_int {
    VFS_INSTANCE.open_write_value_node(node_handle)
}
