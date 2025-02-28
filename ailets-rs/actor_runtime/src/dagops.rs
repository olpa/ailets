use std::iter::Map;

pub trait DagOpsTrait {
    /// # Errors
    /// From the host
    fn value_node(&self, value: &[u8], explain: &str) -> Result<u32, String>;

    /// # Errors
    /// From the host
    fn alias(&self, alias: &str, node_handle: u32) -> Result<u32, String>;

    /// # Errors
    /// From the host
    fn instantiate_with_deps(
        &self,
        workflow_name: &str,
        deps: &Map<String, u32>,
    ) -> Result<u32, String>;
}

pub struct DagOps;

impl DagOpsTrait for DagOps {
    fn value_node(&self, value: &[u8], explain: &str) -> Result<u32, String> {
        println!("dag_value_node: {value:?}, explain: {explain}");
        Ok(0)
    }

    fn alias(&self, alias: &str, node_handle: u32) -> Result<u32, String> {
        println!("dag_alias: {alias}, node_handle: {node_handle}");
        Ok(0)
    }

    fn instantiate_with_deps(
        &self,
        workflow_name: &str,
        deps: &Map<String, u32>,
    ) -> Result<u32, String> {
        println!("dag_instantiate_with_deps: {workflow_name}, deps: {deps:?}");
        Ok(0)
    }
}
