#[link(wasm_import_module = "")]
extern "C" {
    pub fn n_of_streams(name_ptr: *const i8) -> i32;
    pub fn open_read(name_ptr: *const i8, index: usize) -> i32;
    pub fn open_write(name_ptr: *const i8) -> i32;
    pub fn aread(fd: usize, buffer_ptr: *mut u8, count: usize) -> i32;
    pub fn awrite(fd: usize, buffer_ptr: *const u8, count: usize) -> i32;
    pub fn aclose(fd: usize) -> i32;
}
