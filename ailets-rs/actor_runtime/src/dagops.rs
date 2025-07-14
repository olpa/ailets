use crate::actor_runtime::{
    dag_alias, dag_detach_from_alias, dag_instantiate_with_deps, dag_value_node,
    open_write_value_node as raw_open_write_value_node,
};
use base64::engine;
use std::io::Write;
use std::os::raw::c_int;

pub fn value_node(value: &[u8], explain: &str) -> Result<u32, String> {
    let mut value_base64 = Vec::new();
    let mut enc =
        base64::write::EncoderWriter::new(&mut value_base64, &engine::general_purpose::STANDARD);
    enc.write_all(value).map_err(|e| e.to_string())?;
    enc.finish().map_err(|e| e.to_string())?;
    drop(enc);
    value_base64.push(b'\0');

    let explain = std::ffi::CString::new(explain).map_err(|e| e.to_string())?;

    let handle = unsafe { dag_value_node(value_base64.as_ptr(), explain.as_ptr().cast::<i8>()) };
    u32::try_from(handle).map_err(|_| "dag_value_node: error".to_string())
}

pub fn alias(alias: &str, node_handle: u32) -> Result<u32, String> {
    let alias = std::ffi::CString::new(alias).map_err(|e| e.to_string())?;

    #[allow(clippy::cast_possible_wrap)]
    let node_handle = node_handle as i32;

    let handle = unsafe { dag_alias(alias.as_ptr().cast::<i8>(), node_handle) };
    u32::try_from(handle).map_err(|_| "dag_alias: error".to_string())
}

pub fn detach_from_alias(alias: &str) -> Result<(), String> {
    let alias = std::ffi::CString::new(alias).map_err(|e| e.to_string())?;

    let result = unsafe { dag_detach_from_alias(alias.as_ptr().cast::<i8>()) };
    if result == 0 {
        Ok(())
    } else {
        Err("dag_detach_from_alias: error".to_string())
    }
}

pub fn instantiate_with_deps(
    workflow_name: &str,
    deps: impl Iterator<Item = (String, u32)>,
) -> Result<u32, String> {
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
    u32::try_from(handle).map_err(|_| "dag_instantiate_with_deps: error".to_string())
}

pub fn open_write_value_node(node_handle: u32) -> Result<i32, String> {
    let node_handle_i32 =
        c_int::try_from(node_handle).map_err(|_| "Node handle too large for i32".to_string())?;

    let fd = unsafe { raw_open_write_value_node(node_handle_i32) };
    if fd < 0 {
        return Err("Failed to open value node for writing".to_string());
    }

    Ok(fd)
}
