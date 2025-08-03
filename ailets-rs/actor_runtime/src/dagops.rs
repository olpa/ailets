use crate::actor_runtime::{
    dag_alias, dag_alias_fd, dag_detach_from_alias, dag_instantiate_with_deps, dag_value_node,
    open_write_pipe as raw_open_write_pipe,
};
use base64::engine;
use std::io::Write;

/// Creates a value node in the DAG with the provided data.
///
/// # Arguments
///
/// * `value` - The binary data to store in the value node
/// * `explain` - A description or explanation of the value node
///
/// # Returns
///
/// Returns a `Result` containing the node handle on success, or an error message on failure.
///
/// # Errors
///
/// - Normally, should never fail.
pub fn value_node(value: &[u8], explain: &str) -> Result<i32, String> {
    let mut value_base64 = Vec::new();
    let mut enc =
        base64::write::EncoderWriter::new(&mut value_base64, &engine::general_purpose::STANDARD);
    enc.write_all(value).map_err(|e| e.to_string())?;
    enc.finish().map_err(|e| e.to_string())?;
    drop(enc);
    value_base64.push(b'\0');

    let explain = std::ffi::CString::new(explain).map_err(|e| e.to_string())?;

    let handle = unsafe { dag_value_node(value_base64.as_ptr(), explain.as_ptr().cast::<i8>()) };
    Ok(handle)
}

/// Creates an alias for an existing node in the DAG.
///
/// # Arguments
///
/// * `alias` - The alias name to assign to the node
/// * `node_handle` - The handle of the existing node to alias
///
/// # Returns
///
/// Returns a `Result` containing the alias handle on success, or an error message on failure.
///
/// # Errors
///
/// - Wrong handle.
pub fn alias(alias: &str, node_handle: i32) -> Result<i32, String> {
    let alias = std::ffi::CString::new(alias).map_err(|e| e.to_string())?;

    let handle = unsafe { dag_alias(alias.as_ptr().cast::<i8>(), node_handle) };
    Ok(handle)
}

/// Detaches a node from its alias in the DAG.
///
/// # Arguments
///
/// * `alias` - The alias name to detach
///
/// # Returns
///
/// Returns a `Result` indicating success or failure.
///
/// # Errors
///
/// - Normally, should never fail
pub fn detach_from_alias(alias: &str) -> Result<(), String> {
    let alias = std::ffi::CString::new(alias).map_err(|e| e.to_string())?;

    let result = unsafe { dag_detach_from_alias(alias.as_ptr().cast::<i8>()) };
    if result == 0 {
        Ok(())
    } else {
        Err("dag_detach_from_alias: error".to_string())
    }
}

/// Instantiates a workflow with dependencies in the DAG.
///
/// # Arguments
///
/// * `workflow_name` - The name of the workflow to instantiate
/// * `deps` - An iterator of dependencies as (name, handle) pairs
///
/// # Returns
///
/// Returns a `Result` containing the workflow instance handle on success, or an error message on failure.
///
/// # Errors
///
/// - The host can fail
pub fn instantiate_with_deps(
    workflow_name: &str,
    deps: impl Iterator<Item = (String, i32)>,
) -> Result<i32, String> {
    let workflow_name = std::ffi::CString::new(workflow_name).map_err(|e| e.to_string())?;

    let mut deps_json = serde_json::Map::new();
    for (key, value) in deps {
        deps_json.insert(key, serde_json::Value::Number(value.into()));
    }
    let deps_vec = serde_json::to_vec(&deps_json).map_err(|e| e.to_string())?;
    let deps_str = std::ffi::CString::new(deps_vec).map_err(|e| e.to_string())?;

    let handle = unsafe {
        dag_instantiate_with_deps(
            workflow_name.as_ptr().cast::<i8>(),
            deps_str.as_ptr().cast::<i8>(),
        )
    };
    Ok(handle)
}

/// Creates an open value node that can be written to through a file descriptor.
///
/// # Arguments
///
/// * `explain` - Optional description or explanation of the open value node
///
/// # Returns
///
/// Returns a `Result` containing the node handle on success, or an error message on failure.
///
/// # Errors
///
/// - Host runtime error
pub fn open_write_pipe(explain: Option<&str>) -> Result<i32, String> {
    let explain_cstr = if let Some(explain) = explain {
        Some(std::ffi::CString::new(explain).map_err(|e| e.to_string())?)
    } else {
        None
    };

    let explain_ptr = explain_cstr
        .as_ref()
        .map_or(std::ptr::null(), |s| s.as_ptr().cast::<i8>());

    let handle = unsafe { raw_open_write_pipe(explain_ptr) };
    Ok(handle)
}

/// Creates an alias for the node associated with a file descriptor.
///
/// # Arguments
///
/// * `alias` - The alias name to assign to the node associated with the file descriptor
/// * `fd` - The file descriptor whose associated node to create an alias for
///
/// # Returns
///
/// Returns a `Result` containing the alias handle on success, or an error message on failure.
///
/// # Errors
///
/// - Invalid file descriptor
/// - Host runtime error
pub fn alias_fd(alias: &str, fd: i32) -> Result<i32, String> {
    let alias = std::ffi::CString::new(alias).map_err(|e| e.to_string())?;

    let handle = unsafe { dag_alias_fd(alias.as_ptr().cast::<i8>(), fd) };
    Ok(handle)
}
