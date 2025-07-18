use std::os::raw::{c_char, c_int, c_uint};

#[link(wasm_import_module = "")]
extern "C" {
    pub fn get_errno() -> c_int;

    pub fn open_read(name_ptr: *const c_char) -> c_int;
    pub fn open_write(name_ptr: *const c_char) -> c_int;
    pub fn aread(fd: c_int, buffer_ptr: *mut u8, count: c_uint) -> c_int;
    pub fn awrite(fd: c_int, buffer_ptr: *const u8, count: c_uint) -> c_int;
    pub fn aclose(fd: c_int) -> c_int;

    /// `dag_value_node` parameters:
    /// - `value_ptr`: pointer to the base64 encoded value
    /// - `explain_ptr`: pointer to the C-string explanation
    ///
    /// return: handle to the value, or -1 if error
    pub fn dag_value_node(value_ptr: *const u8, explain_ptr: *const c_char) -> c_int;

    /// `dag_alias` parameters:
    /// - `alias_ptr`: pointer to the C-string alias
    /// - `node_handle`: handle to the node
    ///
    /// return: handle to the alias, or -1 if error
    pub fn dag_alias(alias_ptr: *const c_char, node_handle: c_int) -> c_int;

    /// `dag_detach_from_alias` parameters:
    /// - `alias_ptr`: pointer to the C-string alias
    ///
    /// return: 0 if success, -1 if error
    pub fn dag_detach_from_alias(alias_ptr: *const c_char) -> c_int;

    /// `dag_instantiate_with_deps` parameters:
    /// - `workflow`: pointer to the C-string workflow name
    /// - `deps`: pointer to the C-string JSON dependencies map
    ///
    /// return: handle to the workflow, or -1 if error
    pub fn dag_instantiate_with_deps(workflow: *const c_char, deps: *const c_char) -> c_int;

    /// `open_write_pipe` parameters:
    /// - `explain_ptr`: pointer to the C-string explanation (optional)
    ///
    /// return: handle to the open value node, or -1 if error
    pub fn open_write_pipe(explain_ptr: *const c_char) -> c_int;

    /// `dag_alias_fd` parameters:
    /// - `alias_ptr`: pointer to the C-string alias
    /// - `fd`: file descriptor to create an alias for
    ///
    /// return: 0 if success, or -1 if error
    pub fn dag_alias_fd(alias_ptr: *const c_char, fd: c_int) -> c_int;
}
