use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use actor_runtime_mocked::{Vfs, VfsWriter};
use gpt::dagops::DagOpsTrait;
use std::io::Write;

pub struct TrackedDagOps {
    vfs: Rc<RefCell<Vfs>>,
    value_nodes: Vec<String>,
    aliases: Vec<String>,
    detached: Vec<String>,
    workflows: Vec<String>,
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
    fn value_node(&mut self, value: &[u8], explain: &str) -> Result<i32, String> {
        let handle = self.value_nodes.len();
        self.value_nodes.push(format!("{handle}:{explain}"));

        // Create file "value.N" on the VFS with the initial value
        let filename = format!("value.{handle}");
        self.vfs.borrow_mut().add_file(filename, value.to_vec());

        Ok(handle as i32)
    }

    fn alias(&mut self, alias: &str, node_handle: i32) -> Result<i32, String> {
        let handle = self.aliases.len() + self.value_nodes.len() + self.workflows.len();
        self.aliases.push(format!("{handle}:{alias}:{node_handle}"));
        Ok(handle as i32)
    }

    fn instantiate_with_deps(
        &mut self,
        workflow_name: &str,
        deps: impl Iterator<Item = (String, i32)>,
    ) -> Result<i32, String> {
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
        Ok(handle as i32)
    }

    fn detach_from_alias(&mut self, alias: &str) -> Result<(), String> {
        self.detached.push(alias.to_string());
        Ok(())
    }

    fn open_write_pipe(&mut self, explain: Option<&str>) -> Result<i32, String> {
        let handle = self.value_nodes.len();
        let explain_str = explain.unwrap_or("pipe");
        self.value_nodes.push(format!("{handle}:{explain_str}"));

        // Create file "value.N" on the VFS for the pipe
        let filename = format!("value.{handle}");
        self.vfs.borrow_mut().add_file(filename, Vec::new());

        // Return the handle as fd for simplicity in mock
        Ok(handle as i32)
    }

    fn alias_fd(&mut self, alias: &str, fd: i32) -> Result<i32, String> {
        // For mock implementation, create alias directly without generating new handle
        // The format is "{alias_handle}:{alias_name}:{node_handle}"
        // Since we're aliasing an fd (which maps to a value node), use fd as the node_handle
        let alias_handle = self.aliases.len() + self.value_nodes.len() + self.workflows.len();
        self.aliases.push(format!("{alias_handle}:{alias}:{fd}"));
        // Return the original fd
        Ok(fd)
    }

    fn open_writer_to_pipe(&mut self, fd: i32) -> Result<Box<dyn Write>, String> {
        // In mock, fd is the handle
        let filename = format!("value.{fd}");
        let writer = VfsWriter::new(self.vfs.clone(), filename);
        Ok(Box::new(writer))
    }
}

impl TrackedDagOps {
    pub fn value_nodes(&self) -> &Vec<String> {
        &self.value_nodes
    }

    pub fn aliases(&self) -> &Vec<String> {
        &self.aliases
    }

    pub fn detached(&self) -> &Vec<String> {
        &self.detached
    }

    pub fn workflows(&self) -> &Vec<String> {
        &self.workflows
    }

    pub fn parse_value_node(&self, value_node: &str) -> (i32, String, String) {
        let parts = value_node.split(':').collect::<Vec<&str>>();
        assert_eq!(parts.len(), 2);
        let handle = parts[0].parse::<i32>().unwrap();
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

    pub fn parse_workflow(&self, workflow: &str) -> (i32, String, HashMap<String, i32>) {
        let parts = workflow.split(':').collect::<Vec<&str>>();
        assert_eq!(parts.len(), 3);
        let handle = parts[0].parse::<i32>().unwrap();
        let explain = parts[1].to_string();

        let mut deps = HashMap::new();
        let deps_parts: Vec<&str> = parts[2].split(',').filter(|s| !s.is_empty()).collect();
        for chunk in deps_parts.chunks(2) {
            if chunk.len() == 2 {
                if let Ok(value) = chunk[1].parse::<i32>() {
                    deps.insert(chunk[0].to_string(), value);
                }
            }
        }

        (handle, explain, deps)
    }

    pub fn parse_alias(&self, alias: &str) -> (i32, String, i32) {
        let parts = alias.split(':').collect::<Vec<&str>>();
        assert_eq!(parts.len(), 3);
        let node_handle = parts[0].parse::<i32>().unwrap();
        let alias_name = parts[1].to_string();
        let alias_handle = parts[2].parse::<i32>().unwrap();

        (node_handle, alias_name, alias_handle)
    }
}
