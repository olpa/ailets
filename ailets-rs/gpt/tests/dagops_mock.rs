use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use actor_runtime::DagOpsTrait;
use gpt::dagops::{inject_tool_calls, InjectDagOpsTrait};
use gpt::funcalls::ContentItemFunction;

pub struct TrackedInjectDagOps {
    dagops: Rc<RefCell<TrackedDagOps>>,
    tool_calls: Vec<ContentItemFunction>,
}

impl TrackedInjectDagOps {
    #[allow(clippy::new_without_default)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            dagops: Rc::new(RefCell::new(TrackedDagOps::default())),
            tool_calls: Vec::new(),
        }
    }

    #[must_use]
    pub fn get_tool_calls(&self) -> &Vec<ContentItemFunction> {
        &self.tool_calls
    }
}

impl InjectDagOpsTrait for TrackedInjectDagOps {
    fn inject_tool_calls(&mut self, tool_calls: &[ContentItemFunction]) -> Result<(), String> {
        self.tool_calls = tool_calls.to_vec();
        inject_tool_calls(&mut *self.dagops.borrow_mut(), tool_calls)
    }
}

#[derive(Debug, Default)]
pub struct TrackedDagOps {
    pub value_nodes: Rc<RefCell<Vec<String>>>,
    pub aliases: Vec<String>,
    pub detached: Vec<String>,
    pub workflows: Vec<String>,
}

impl DagOpsTrait for TrackedDagOps {
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<u32, String> {
        let handle = self.value_nodes.borrow().len();
        self.value_nodes
            .borrow_mut()
            .push(format!("{handle}:{explain}:{value:?}"));
        let new_len = self.value_nodes.borrow().len();
        eprintln!("value_node: pushed, new_len={}, handle={}", new_len, handle);
        Ok(handle as u32)
    }

    fn alias(&mut self, alias: &str, node_handle: u32) -> Result<u32, String> {
        let handle = self.aliases.len() + self.value_nodes.borrow().len() + self.workflows.len();
        self.aliases.push(format!("{handle}:{alias}:{node_handle}"));
        Ok(handle as u32)
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
        let handle = self.workflows.len() + self.aliases.len() + self.value_nodes.borrow().len();
        self.workflows
            .push(format!("{handle}:{workflow_name}:{deps_str}"));
        Ok(handle as u32)
    }

    fn detach_from_alias(&mut self, alias: &str) -> Result<(), String> {
        self.detached.push(alias.to_string());
        Ok(())
    }

    fn writer_for_value_node(
        &mut self,
        node_handle: u32,
    ) -> Result<Box<dyn std::io::Write>, String> {
        use std::io::Write;

        let idx = node_handle as usize;
        let current_len = self.value_nodes.borrow().len();
        eprintln!(
            "writer_for_value_node: node_handle={}, value_nodes.len()={:?}",
            idx, current_len
        );
        if idx >= current_len {
            eprintln!("Invalid node handle: {} >= {}", idx, current_len);
            return Err("Invalid node handle".to_string());
        }

        // Create a shared reference to the value_nodes vector (no lifetime issues)
        let value_nodes_ref = Rc::clone(&self.value_nodes);

        struct MockValueWriter {
            value_nodes: Rc<RefCell<Vec<String>>>,
            idx: usize,
        }

        impl std::io::Write for MockValueWriter {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                let prefix = {
                    let value_nodes = self.value_nodes.borrow();
                    let value_node = &value_nodes[self.idx];
                    value_node.rsplit_once(':').map(|(p, _)| p.to_string())
                };
                if let Some(prefix) = prefix {
                    let mut value_nodes = self.value_nodes.borrow_mut();
                    value_nodes[self.idx] =
                        format!("{}:{}", prefix, std::str::from_utf8(buf).unwrap());
                }
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        Ok(Box::new(MockValueWriter {
            value_nodes: value_nodes_ref,
            idx,
        }))
    }
}

impl TrackedDagOps {
    pub fn parse_value_node(&self, value_node: &str) -> (u32, String, String) {
        let parts = value_node.split(':').collect::<Vec<&str>>();
        assert_eq!(parts.len(), 3);
        let handle = parts[0].parse::<u32>().unwrap();
        let explain = parts[1].to_string();

        let value = parts[2].to_string();
        let bytes: Vec<u8> = serde_json::from_str(&value).unwrap();
        let value = String::from_utf8(bytes).unwrap();

        (handle, explain, value)
    }

    pub fn parse_workflow(&self, workflow: &str) -> (u32, String, HashMap<String, u32>) {
        let parts = workflow.split(':').collect::<Vec<&str>>();
        assert_eq!(parts.len(), 3);
        let handle = parts[0].parse::<u32>().unwrap();
        let explain = parts[1].to_string();

        let mut deps = HashMap::new();
        let deps_parts: Vec<&str> = parts[2].split(',').filter(|s| !s.is_empty()).collect();
        for chunk in deps_parts.chunks(2) {
            if chunk.len() == 2 {
                if let Ok(value) = chunk[1].parse::<u32>() {
                    deps.insert(chunk[0].to_string(), value);
                }
            }
        }

        (handle, explain, deps)
    }

    pub fn parse_alias(&self, alias: &str) -> (u32, String, u32) {
        let parts = alias.split(':').collect::<Vec<&str>>();
        assert_eq!(parts.len(), 3);
        let node_handle = parts[0].parse::<u32>().unwrap();
        let alias_name = parts[1].to_string();
        let alias_handle = parts[2].parse::<u32>().unwrap();

        (node_handle, alias_name, alias_handle)
    }
}

impl Clone for TrackedDagOps {
    fn clone(&self) -> Self {
        TrackedDagOps {
            value_nodes: Rc::clone(&self.value_nodes),
            aliases: self.aliases.clone(),
            detached: self.detached.clone(),
            workflows: self.workflows.clone(),
        }
    }
}
