use std::os::raw::{c_char, c_int, c_uint};

#[link(wasm_import_module = "")]
extern "C" {
    pub fn n_of_streams(name_ptr: *const c_char) -> c_int;
    pub fn open_read(name_ptr: *const c_char, index: c_uint) -> c_int;
    pub fn open_write(name_ptr: *const c_char) -> c_int;
    pub fn aread(fd: c_int, buffer_ptr: *mut u8, count: c_uint) -> c_int;
    pub fn awrite(fd: c_int, buffer_ptr: *const u8, count: c_uint) -> c_int;
    pub fn aclose(fd: c_int) -> c_int;

    #[cfg(feature = "dagops")]
    pub fn dag_value_node(value_ptr: *const u8, explain_ptr: *const c_char) -> c_int;
    #[cfg(feature = "dagops")]
    pub fn dag_alias(alias: *const c_char, node_handle: c_int) -> c_int;
    #[cfg(feature = "dagops")]
    pub fn dag_instantiate_with_deps(
        workflow: *const c_char,
        deps: *const u8, // later to be replaced with a map using UniFFI
    ) -> c_int;
}
