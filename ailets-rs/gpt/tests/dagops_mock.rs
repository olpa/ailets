use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use actor_runtime_mocked::{Vfs, VfsWriter};
use gpt::dagops::DagOpsTrait;
use gpt::dagops::{inject_tool_calls, InjectDagOpsTrait};
use gpt::funcalls::ContentItemFunction;
use std::io::Write;

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

pub struct TrackedDagOps {
    pub vfs: Rc<RefCell<Vfs>>,
    pub value_nodes: Vec<String>,
    pub aliases: Vec<String>,
    pub detached: Vec<String>,
    pub workflows: Vec<String>,
}

impl Default for TrackedDagOps {
    fn default() -> Self {
        Self {
            vfs: Rc::new(RefCell::new(Vfs::new())),
            value_nodes: Vec::new(),
            aliases: Vec::new(),
            detached: Vec::new(),
            workflows: Vec::new(),
        }
    }
}

impl DagOpsTrait for TrackedDagOps {
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<u32, String> {
        let handle = self.value_nodes.len();
        self.value_nodes.push(format!("{handle}:{explain}"));

        // Create file "value.N" on the VFS with the initial value
        let filename = format!("value.{handle}");
        self.vfs.borrow_mut().add_file(filename, value.to_vec());

        eprintln!(
            "value_node: created, handle={}, explain={}",
            handle, explain
        );
        Ok(handle as u32)
    }

    fn alias(&mut self, alias: &str, node_handle: u32) -> Result<u32, String> {
        let handle = self.aliases.len() + self.value_nodes.len() + self.workflows.len();
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
        let handle = self.workflows.len() + self.aliases.len() + self.value_nodes.len();
        self.workflows
            .push(format!("{handle}:{workflow_name}:{deps_str}"));
        Ok(handle as u32)
    }

    fn detach_from_alias(&mut self, alias: &str) -> Result<(), String> {
        self.detached.push(alias.to_string());
        Ok(())
    }

    fn open_write_value_node(&mut self, node_handle: u32) -> Result<i32, String> {
        let idx = node_handle as usize;
        let current_len = self.value_nodes.len();
        eprintln!(
            "open_write_value_node: node_handle={}, value_nodes.len()={:?}",
            idx, current_len
        );
        if idx >= current_len {
            eprintln!("Invalid node handle: {} >= {}", idx, current_len);
            return Err("Invalid node handle".to_string());
        }

        // Use VFS to open the value file for writing
        let fd = self.vfs.borrow().open_write_value_node(node_handle as i32);
        if fd < 0 {
            return Err("Failed to open value node for writing".to_string());
        }
        Ok(fd)
    }

    fn open_writer_to_value_node(&mut self, node_handle: u32) -> Result<Box<dyn Write>, String> {
        let filename = format!("value.{node_handle}");
        let writer = VfsWriter::new(self.vfs.clone(), filename);
        Ok(Box::new(writer))
    }
}

impl TrackedDagOps {
    pub fn parse_value_node(&self, value_node: &str) -> (u32, String, String) {
        let parts = value_node.split(':').collect::<Vec<&str>>();
        assert_eq!(parts.len(), 2);
        let handle = parts[0].parse::<u32>().unwrap();
        let explain = parts[1].to_string();

        // Get content from value.N file in VFS
        let filename = format!("value.{handle}");
        let content = self
            .vfs
            .borrow()
            .get_file(&filename)
            .unwrap_or_else(|_| Vec::new());
        let value = String::from_utf8(content).unwrap_or_default();

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
            vfs: self.vfs.clone(), // Share the same VFS instance
            value_nodes: self.value_nodes.clone(),
            aliases: self.aliases.clone(),
            detached: self.detached.clone(),
            workflows: self.workflows.clone(),
        }
    }
}
