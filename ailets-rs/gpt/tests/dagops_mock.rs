use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use actor_runtime_mocked::{Vfs, VfsWriter};
use gpt::dagops::{DagOpsTrait, DagOpsWrite, InjectDagOpsTrait};
use gpt::funcalls::FunCallsWrite;
use std::io::Write;

/// Test-only definition of ContentItemFunction for backward compatibility
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct ContentItemFunction {
    // type: "function",
    pub id: String,
    pub function_name: String,
    pub function_arguments: String,
}

impl ContentItemFunction {
    /// Creates a new function call
    #[must_use]
    pub fn new(id: &str, function_name: &str, function_arguments: &str) -> Self {
        Self {
            id: id.to_string(),
            function_name: function_name.to_string(),
            function_arguments: function_arguments.to_string(),
        }
    }
}

/// Test-only function: Inject function calls into the workflow DAG using FunCallsWrite.
pub fn inject_tool_calls(
    dagops: &mut impl DagOpsTrait,
    tool_calls: &[ContentItemFunction],
) -> Result<(), String> {
    if tool_calls.is_empty() {
        return Ok(());
    }

    // Use DagOpsWrite for the actual implementation
    let mut writer = DagOpsWrite::new(dagops);

    for (index, tool_call) in tool_calls.iter().enumerate() {
        writer
            .new_item(tool_call.id.clone(), tool_call.function_name.clone())
            .map_err(|e| e.to_string())?;
        writer
            .arguments_chunk(tool_call.function_arguments.clone())
            .map_err(|e| e.to_string())?;
        writer.end_item().map_err(|e| e.to_string())?;
    }

    writer.end().map_err(|e| e.to_string())?;

    Ok(())
}

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
    fn process_with_funcalls_write(
        &mut self,
        _writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Not needed for this mock
        Ok(())
    }
}

// Add a test-specific method for backward compatibility
impl TrackedInjectDagOps {
    pub fn inject_tool_calls(&mut self, tool_calls: &[ContentItemFunction]) -> Result<(), String> {
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
    pub next_fd: i32,
}

impl Default for TrackedDagOps {
    fn default() -> Self {
        Self {
            vfs: Rc::new(RefCell::new(Vfs::new())),
            value_nodes: Vec::new(),
            aliases: Vec::new(),
            detached: Vec::new(),
            workflows: Vec::new(),
            next_fd: 10, // Start at 10 to avoid conflicts with standard fds
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

    fn alias_fd(&mut self, alias: &str, fd: u32) -> Result<u32, String> {
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
            next_fd: self.next_fd,
        }
    }
}
