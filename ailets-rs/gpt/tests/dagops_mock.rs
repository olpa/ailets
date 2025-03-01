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
        
        // Create chat history value node
        let mut dagops = self.dagops.borrow_mut();
        dagops.value_node(b"tool_calls", "Feed \"tool_calls\" from output to input")?;
        dagops.alias(".chat_messages", 0)?;

        // Process each tool call
        for tool_call in funcalls.get_tool_calls() {
            // Create tool spec value node
            let tool_spec_handle = dagops.value_node(
                tool_call.function_arguments.as_bytes(),
                "Tool call spec from llm",
            )?;

            // Instantiate tool workflow
            let tool_name = &tool_call.function_name;
            let tool_handle = dagops.instantiate_with_deps(
                &format!(".tool.{tool_name}"),
                &HashMap::from([(".tool_input".to_string(), tool_spec_handle)]),
            )?;

            // Convert tool output to messages
            let msg_handle = dagops.instantiate_with_deps(
                ".toolcall_to_messages",
                &HashMap::from([
                    (".llm_tool_spec".to_string(), tool_spec_handle),
                    (".tool_output".to_string(), tool_handle),
                ]),
            )?;
            dagops.alias(".chat_messages", msg_handle)?;
        }

        // Rerun model
        let rerun_handle = dagops.instantiate_with_deps(".gpt4o", &HashMap::new())?;
        dagops.alias(".model_output", rerun_handle)?;

        Ok(())
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
