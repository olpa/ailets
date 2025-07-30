use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use actor_runtime_mocked::{Vfs, VfsWriter};
use gpt::dagops::DagOpsTrait;
use std::io::Write;

/// Writer that intercepts tool spec writes to extract tool call information
struct ToolSpecTrackingWriter {
    inner: Box<dyn Write>,
    dagops_ref: *mut TrackedDagOps,
    buffer: String,
}

impl ToolSpecTrackingWriter {
    fn new(inner: Box<dyn Write>, dagops_ref: *mut TrackedDagOps) -> Self {
        Self { 
            inner, 
            dagops_ref, 
            buffer: String::new() 
        }
    }
}

impl Write for ToolSpecTrackingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let data = String::from_utf8_lossy(buf);
        self.buffer.push_str(&data);
        
        // Try to parse the complete JSON when we see the closing pattern
        if self.buffer.contains(r#"}]"#) && self.buffer.starts_with(r#"[{"type":"function","id":"#) {
            // Extract ID and name from the JSON
            if let Some(id_start) = self.buffer.find(r#""id":""#) {
                let id_start = id_start + 6;  // Skip `"id":"`
                if let Some(id_end) = self.buffer[id_start..].find('"') {
                    let id = &self.buffer[id_start..id_start + id_end];
                    
                    if let Some(name_start) = self.buffer.find(r#""name":""#) {
                        let name_start = name_start + 8;  // Skip `"name":"`
                        if let Some(name_end) = self.buffer[name_start..].find('"') {
                            let name = &self.buffer[name_start..name_start + name_end];
                            
                            // Extract arguments - they're between `"arguments":"` and `"}]`
                            let mut arguments = String::new();
                            if let Some(args_start) = self.buffer.find(r#""arguments":""#) {
                                let args_start = args_start + 13;  // Skip `"arguments":"`
                                if let Some(args_end) = self.buffer[args_start..].find(r#""}"#) {
                                    arguments = self.buffer[args_start..args_start + args_end].to_string();
                                }
                            }
                            
                            // Safely update the TrackedDagOps
                            unsafe {
                                let dagops = &mut *self.dagops_ref;
                                if let Some(mut current_call) = dagops.current_tool_call.take() {
                                    current_call.id = id.to_string();
                                    current_call.function_name = name.to_string();
                                    current_call.function_arguments = arguments;
                                    dagops.current_tool_call = Some(current_call);
                                } else {
                                    let new_call = ContentItemFunction {
                                        id: id.to_string(),
                                        function_name: name.to_string(),
                                        function_arguments: arguments,
                                    };
                                    dagops.current_tool_call = Some(new_call);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

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


pub struct TrackedDagOps {
    pub vfs: Rc<RefCell<Vfs>>,
    pub value_nodes: Vec<String>,
    pub aliases: Vec<String>,
    pub detached: Vec<String>,
    pub workflows: Vec<String>,
    pub next_fd: i32,
    pub tool_calls: Vec<ContentItemFunction>,
    current_tool_call: Option<ContentItemFunction>,
    tool_spec_fd: Option<i32>,
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
            tool_calls: Vec::new(),
            current_tool_call: None,
            tool_spec_fd: None,
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
        let deps_vec: Vec<_> = deps.collect();
        for (key, value) in &deps_vec {
            deps_str.push_str(key.as_str());
            deps_str.push(',');
            deps_str.push_str(&value.to_string());
            deps_str.push(',');
        }
        
        // Track tool instantiation
        if workflow_name.starts_with(".tool.") {
            let tool_name = &workflow_name[6..]; // Remove ".tool." prefix
            if let Some(mut current_call) = self.current_tool_call.take() {
                current_call.function_name = tool_name.to_string();
                self.current_tool_call = Some(current_call);
            } else {
                // Create a new tool call if we don't have one
                let mut new_call = ContentItemFunction::default();
                new_call.function_name = tool_name.to_string();
                self.current_tool_call = Some(new_call);
            }
        }
        
        // Finalize tool call when toolcall_to_messages is instantiated
        if workflow_name == ".toolcall_to_messages" {
            if let Some(current_call) = self.current_tool_call.take() {
                self.tool_calls.push(current_call);
            }
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
        // Track tool spec fd for later use
        if alias == ".llm_tool_spec" {
            self.tool_spec_fd = Some(fd as i32);
        }
        
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
        
        // If this is the tool spec fd, wrap it with tracking
        if Some(fd) == self.tool_spec_fd {
            let tracking_writer = ToolSpecTrackingWriter::new(
                Box::new(writer),
                self as *mut TrackedDagOps
            );
            Ok(Box::new(tracking_writer))
        } else {
            Ok(Box::new(writer))
        }
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
            tool_calls: self.tool_calls.clone(),
            current_tool_call: self.current_tool_call.clone(),
            tool_spec_fd: self.tool_spec_fd,
        }
    }
}

impl TrackedDagOps {
    #[must_use]
    pub fn get_tool_calls(&self) -> &Vec<ContentItemFunction> {
        &self.tool_calls
    }

}

