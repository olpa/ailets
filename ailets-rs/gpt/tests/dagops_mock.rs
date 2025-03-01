use std::cell::RefCell;

use actor_runtime::DagOpsTrait;
use gpt::dagops::InjectDagOpsTrait;
use gpt::funcalls::{ContentItemFunction, FunCalls};
use gpt::dagops::inject_tool_calls_to_dagops;

pub struct TrackedInjectDagOps {
    dagops: RefCell<TrackedDagOps>,
    tool_calls: Vec<ContentItemFunction>,
}

impl TrackedInjectDagOps {
    #[allow(clippy::new_without_default)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            dagops: RefCell::new(TrackedDagOps::default()),
            tool_calls: Vec::new(),
        }
    }

    #[must_use]
    pub fn get_tool_calls(&self) -> &Vec<ContentItemFunction> {
        &self.tool_calls
    }
}

impl InjectDagOpsTrait for TrackedInjectDagOps {
    fn inject_tool_calls(&mut self, tool_calls: &Vec<ContentItemFunction>) -> Result<(), String> {
        self.tool_calls = tool_calls.clone();
        inject_tool_calls_to_dagops(&mut *self.dagops.borrow_mut(), tool_calls)
    }
}

#[derive(Debug, Default, Clone)]
pub struct TrackedDagOps {
    pub value_nodes: Vec<String>,
    pub aliases: Vec<String>,
    pub workflows: Vec<String>,
}

impl DagOpsTrait for TrackedDagOps {
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<u32, String> {
        self.value_nodes.push(format!("{explain}:{value:?}"));
        Ok((self.value_nodes.len() + self.aliases.len() + self.workflows.len()) as u32)
    }

    fn alias(&mut self, alias: &str, node_handle: u32) -> Result<u32, String> {
        self.aliases.push(format!("{alias}:{node_handle}"));
        Ok((self.aliases.len() + self.value_nodes.len() + self.workflows.len()) as u32)
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
        self.workflows.push(format!("{workflow_name}:{deps_str}"));
        Ok((self.workflows.len() + self.aliases.len() + self.value_nodes.len()) as u32)
    }
}
