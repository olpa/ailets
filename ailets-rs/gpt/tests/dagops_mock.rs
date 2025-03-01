use std::cell::RefCell;
use std::collections::HashMap;
use std::iter::Map;

use gpt::dagops::InjectDagOpsTrait;
use gpt::funcalls::{ContentItemFunction, FunCalls};

pub struct TrackedInjectDagOps {
    funcalls: RefCell<FunCalls>,
    dagops: RefCell<TrackedDagOps>,
}

impl TrackedInjectDagOps {
    #[allow(clippy::new_without_default)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            funcalls: RefCell::new(FunCalls::new()),
            dagops: RefCell::new(TrackedDagOps::default()),
        }
    }

    pub fn get_funcalls(&self) -> Vec<ContentItemFunction> {
        self.funcalls.borrow().get_tool_calls().clone()
    }

    pub fn get_dagops(&self) -> TrackedDagOps {
        self.dagops.borrow().clone()
    }
}

impl InjectDagOpsTrait for TrackedInjectDagOps {
    fn inject_funcalls(&self, funcalls: &FunCalls) -> Result<(), String> {
        *self.funcalls.borrow_mut() = funcalls.clone();
        inject_funcalls_to_dagops(&*self.dagops.borrow_mut(), funcalls)
    }
}

#[derive(Debug, Default, Clone)]
pub struct TrackedDagOps {
    pub value_nodes: Vec<String>,
    pub aliases: Vec<String>,
    pub workflows: Vec<String>,
}

impl TrackedDagOps {
    pub fn value_node(&mut self, value: &[u8], explain: &str) -> Result<u32, String> {
        self.value_nodes.push(format!("{explain}:{value:?}"));
        Ok(self.value_nodes.len() + self.aliases.len() + self.workflows.len())
    }

    pub fn alias(&mut self, alias: &str, node_handle: u32) -> Result<u32, String> {
        self.aliases.push(format!("{alias}:{node_handle}"));
        Ok(self.aliases.len() + self.value_nodes.len() + self.workflows.len())
    }

    pub fn instantiate_with_deps(
        &mut self,
        workflow_name: &str,
        deps: &HashMap<String, u32>,
    ) -> Result<u32, String> {
        self.workflows.push(format!("{workflow_name}:{deps:?}"));
        Ok(self.workflows.len() + self.aliases.len() + self.value_nodes.len())
    }
}
