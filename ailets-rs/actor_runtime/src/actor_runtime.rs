use std::os::raw::{c_char, c_int, c_uint};

#[link(wasm_import_module = "")]
extern "C" {
    pub fn n_of_streams(name_ptr: *const c_char) -> c_int;
    pub fn open_read(name_ptr: *const c_char, index: c_uint) -> c_int;
    pub fn open_write(name_ptr: *const c_char) -> c_int;
    pub fn aread(fd: c_int, buffer_ptr: *mut u8, count: c_uint) -> c_int;
    pub fn awrite(fd: c_int, buffer_ptr: *const u8, count: c_uint) -> c_int;
    pub fn aclose(fd: c_int) -> c_int;
}
