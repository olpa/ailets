use crate::actor_runtime::{dag_alias, dag_instantiate_with_deps, dag_value_node};
use std::iter::Iterator;

pub trait DagOpsTrait {
    /// # Errors
    /// From the host
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<u32, String>;

    /// # Errors
    /// From the host
    fn alias(&mut self, alias: &str, node_handle: u32) -> Result<u32, String>;

    /// # Errors
    /// From the host
    fn instantiate_with_deps(
        &mut self,
        workflow_name: &str,
        deps: impl Iterator<Item = (String, u32)>,
    ) -> Result<u32, String>;
}

pub struct DagOps;

impl DagOpsTrait for DagOps {
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<u32, String> {
        println!("dag_value_node: {value:?}, explain: {explain}");
        unsafe { dag_value_node(value.as_ptr(), explain.as_ptr().cast::<i8>()) };
        Ok(0)
    }

    fn alias(&mut self, alias: &str, node_handle: u32) -> Result<u32, String> {
        println!("dag_alias: {alias}, node_handle: {node_handle}");
        #[allow(clippy::cast_possible_wrap)]
        let node_handle = node_handle as i32;
        unsafe { dag_alias(alias.as_ptr().cast::<i8>(), node_handle) };
        Ok(0)
    }

    fn instantiate_with_deps(
        &mut self,
        workflow_name: &str,
        deps: impl Iterator<Item = (String, u32)>,
    ) -> Result<u32, String> {
        let mut deps_str = String::new();
        for (key, value) in deps {
            deps_str.push_str(key.as_str());
            deps_str.push(',');
            deps_str.push_str(&value.to_string());
            deps_str.push(',');
        }
        println!("dag_instantiate_with_deps: {workflow_name}, deps: {deps_str}");
        unsafe {
            dag_instantiate_with_deps(
                workflow_name.as_ptr().cast::<i8>(),
                deps_str.as_bytes().as_ptr().cast::<u8>(),
            )
        };
        Ok(0)
    }
}
