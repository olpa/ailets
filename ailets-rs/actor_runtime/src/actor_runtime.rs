use std::os::raw::c_char;

#[link(wasm_import_module = "")]
extern "C" {
    pub fn n_of_streams(name_ptr: *const c_char) -> i32;
    pub fn open_read(name_ptr: *const c_char, index: usize) -> i32;
    pub fn open_write(name_ptr: *const c_char) -> i32;
    pub fn aread(fd: i32, buffer_ptr: *mut u8, count: usize) -> i32;
    pub fn awrite(fd: i32, buffer_ptr: *const u8, count: usize) -> i32;
    pub fn aclose(fd: i32) -> i32;
}
