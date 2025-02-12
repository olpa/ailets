#[link(wasm_import_module = "")]
extern "C" {
    pub fn n_of_streams(name_ptr: *const u8) -> u32;
    pub fn open_read(name_ptr: *const u8, index: u32) -> u32;
    pub fn open_write(name_ptr: *const u8) -> u32;
    pub fn aread(fd: u32, buffer_ptr: *mut u8, count: u32) -> u32;
    pub fn awrite(fd: u32, buffer_ptr: *const u8, count: u32) -> u32;
    pub fn aclose(fd: u32);
}
