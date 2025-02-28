use std::cell::RefCell;
use std::iter::Map;

use gpt::dagops::InjectDagOpsTrait;
use gpt::funcalls::{ContentItemFunction, FunCalls};

pub struct TrackedInjectDagOps {
    funcalls: RefCell<FunCalls>,
}

impl TrackedInjectDagOps {
    #[allow(clippy::new_without_default)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            funcalls: RefCell::new(FunCalls::new()),
        }
    }

    pub fn get_funcalls(&self) -> Vec<ContentItemFunction> {
        self.funcalls.borrow().get_tool_calls().clone()
    }
}

impl InjectDagOpsTrait for TrackedInjectDagOps {
    fn inject_funcalls(&self, funcalls: &FunCalls) -> Result<(), String> {
        *self.funcalls.borrow_mut() = funcalls.clone();
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct TrackedDagOps {
    pub value_nodes: Vec<String>,
    pub aliases: Vec<String>,
    pub workflows: Vec<String>,
}

impl TrackedDagOps {
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<u32, String> {
        self.value_nodes.push(format!("{explain}:{value:?}"));
        Ok(0)
    }

    fn alias(&mut self, alias: &str, node_handle: u32) -> Result<u32, String> {
        self.aliases.push(format!("{alias}:{node_handle}"));
        Ok(0)
    }

    fn instantiate_with_deps(
        &mut self,
        workflow_name: &str,
        deps: &Map<String, u32>,
    ) -> Result<u32, String> {
        self.workflows.push(format!("{workflow_name}:{deps:?}"));
        Ok(0)
    }
}
