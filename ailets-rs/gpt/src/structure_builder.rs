//! A module for building structured messages in a streaming fashion.
//!
//! Collects function calls from the JSON stream and stores them in a `FunCalls` struct.

use crate::funcalls::{FunCalls, FunCallsToChat};
use std::io::Write;

pub struct StructureBuilder<W: Write> {
    writer: W,
    role: Option<String>,
    message_has_content: bool,
    text_is_open: bool,
    message_is_closed: bool,
    funcalls: FunCalls,
}

impl<W: Write> StructureBuilder<W> {
    #[must_use]
    pub fn new(writer: W) -> Self {
        StructureBuilder {
            writer,
            role: None,
            message_has_content: false,
            text_is_open: false,
            message_is_closed: false,
            funcalls: FunCalls::new(),
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.writer
    }

    #[must_use]
    pub fn get_funcalls(&self) -> &FunCalls {
        &self.funcalls
    }

    pub fn get_funcalls_mut(&mut self) -> &mut FunCalls {
        &mut self.funcalls
    }

    pub fn begin_message(&mut self) {
        self.role = None;
        self.message_has_content = false;
        self.text_is_open = false;
        self.message_is_closed = false;
        self.funcalls.reset_streaming_state();
    }

    /// End the current message.
    /// # Errors
    /// I/O
    pub fn end_message(&mut self) -> Result<(), std::io::Error> {
        if !self.message_has_content || self.message_is_closed {
            return Ok(());
        }
        if self.text_is_open {
            self.writer.write_all(b"\"}]")?;
            self.text_is_open = false;
            self.writer.write_all(b"\n")?;
        }

        // Finalize any pending tool calls
        if let Some(current) = self.funcalls.get_current_funcall() {
            let formatted_args = if current.function_arguments.starts_with('{')
                && current.function_arguments.ends_with('}')
            {
                current.function_arguments.clone()
            } else {
                format!("{{{}}}", current.function_arguments)
            };
            writeln!(
                self.writer,
                r#"[{{"type":"tool_call"}},{{"id":"{}","function_name":"{}","function_arguments":"{}"}}]"#,
                current.id, current.function_name, formatted_args
            )?;
        }
        self.funcalls.end_current_no_write();

        self.message_is_closed = true;
        Ok(())
    }

    /// Add a role to the current message.
    /// # Errors
    /// I/O
    pub fn role(&mut self, role: &str) -> Result<(), std::io::Error> {
        if self.role.is_some() {
            return Ok(());
        }
        self.role = Some(role.to_owned());
        Ok(())
    }

    /// Write a message boilerplate with "role" (completed) and "content" (open) keys
    /// # Errors
    /// I/O
    pub fn begin_content(&mut self) -> Result<(), std::io::Error> {
        if self.message_has_content {
            return Ok(());
        }
        self.writer.write_all(b"[{\"type\":\"ctl\"},{\"role\":\"")?;
        if let Some(role) = &self.role {
            self.writer.write_all(role.as_bytes())?;
        } else {
            self.writer.write_all(b"assistant")?;
        }
        self.writer.write_all(b"\"}]\n")?;
        self.message_has_content = true;
        self.text_is_open = false;
        Ok(())
    }

    /// Add a text chunk to the current message.
    /// # Errors
    /// I/O
    pub fn begin_text_chunk(&mut self) -> Result<(), std::io::Error> {
        if !self.message_has_content {
            self.begin_content()?;
        }
        if !self.text_is_open {
            self.writer
                .write_all(b"[{\"type\":\"text\"},{\"text\":\"")?;
            self.text_is_open = true;
        }
        Ok(())
    }

    /// Output just the tool call ID as soon as it's available
    /// # Errors
    /// I/O
    pub fn output_tool_call_id(&mut self, id: &str) -> Result<(), std::io::Error> {
        if !self.message_has_content {
            self.begin_content()?;
        }
        if self.text_is_open {
            self.writer.write_all(b"\"}]")?;
            self.text_is_open = false;
        }

        self.writer
            .write_all(b"[{\"type\":\"tool_call_id\"},{\"id\":\"")?;
        self.writer.write_all(id.as_bytes())?;
        self.writer.write_all(b"\"}]\n")?;
        Ok(())
    }

    /// Output just the tool call name as soon as it's available
    /// # Errors
    /// I/O
    pub fn output_tool_call_name(&mut self, name: &str) -> Result<(), std::io::Error> {
        if !self.message_has_content {
            self.begin_content()?;
        }
        if self.text_is_open {
            self.writer.write_all(b"\"}]")?;
            self.text_is_open = false;
        }

        self.writer
            .write_all(b"[{\"type\":\"tool_call_name\"},{\"function_name\":\"")?;
        self.writer.write_all(name.as_bytes())?;
        self.writer.write_all(b"\"}]\n")?;
        Ok(())
    }

    /// Output tool call arguments chunk as it becomes available
    /// # Errors
    /// I/O
    pub fn output_tool_call_arguments_chunk(
        &mut self,
        args_chunk: &str,
    ) -> Result<(), std::io::Error> {
        if !self.message_has_content {
            self.begin_content()?;
        }
        if self.text_is_open {
            self.writer.write_all(b"\"}]")?;
            self.text_is_open = false;
        }

        // Escape the JSON string content for embedding in JSON
        let escaped_args = args_chunk.replace('\\', "\\\\").replace('"', "\\\"");

        self.writer
            .write_all(b"[{\"type\":\"tool_call_arguments\"},{\"function_arguments\":\"")?;
        self.writer.write_all(escaped_args.as_bytes())?;
        self.writer.write_all(b"\"}]\n")?;
        Ok(())
    }

    /// Public interface for setting tool call ID - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_id(&mut self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Handle StructureBuilder-specific logic for switching item types
        if !self.message_has_content {
            self.begin_content()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        if self.text_is_open {
            self.writer
                .write_all(b"\"}]")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            self.text_is_open = false;
        }

        // Create FunCallsToChat writer and forward to funcalls
        let mut chat_writer = FunCallsToChat::new_no_ctl(&mut self.writer);
        self.funcalls.id(id, &mut chat_writer)?;

        Ok(())
    }

    /// Public interface for setting tool call name - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_name(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Handle StructureBuilder-specific logic for switching item types
        if !self.message_has_content {
            self.begin_content()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        if self.text_is_open {
            self.writer
                .write_all(b"\"}]")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            self.text_is_open = false;
        }

        // Create FunCallsToChat writer and forward to funcalls
        let mut chat_writer = FunCallsToChat::new_no_ctl(&mut self.writer);
        self.funcalls.name(name, &mut chat_writer)?;

        Ok(())
    }

    /// Public interface for adding tool call arguments chunk - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if I/O error occurs
    pub fn tool_call_arguments_chunk(
        &mut self,
        args: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Handle StructureBuilder-specific logic for switching item types
        if !self.message_has_content {
            self.begin_content()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        if self.text_is_open {
            self.writer
                .write_all(b"\"}]")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            self.text_is_open = false;
        }

        // Create FunCallsToChat writer and forward to funcalls
        let mut chat_writer = FunCallsToChat::new_no_ctl(&mut self.writer);
        self.funcalls.arguments_chunk(args, &mut chat_writer)?;

        Ok(())
    }

    /// Public interface for setting tool call index - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_index(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        // Handle StructureBuilder-specific logic for switching item types
        if !self.message_has_content {
            self.begin_content()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        if self.text_is_open {
            self.writer
                .write_all(b"\"}]")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            self.text_is_open = false;
        }

        // If index is changing and we have a current tool call, write it before starting the new one
        if let (Some(last_index), Some(current)) = (
            self.funcalls.last_index,
            self.funcalls.get_current_funcall(),
        ) {
            if index > last_index {
                let formatted_args = if current.function_arguments.starts_with('{')
                    && current.function_arguments.ends_with('}')
                {
                    current.function_arguments.clone()
                } else {
                    format!("{{{}}}", current.function_arguments)
                };
                writeln!(
                    self.writer,
                    r#"[{{"type":"tool_call"}},{{"id":"{}","function_name":"{}","function_arguments":"{}"}}]"#,
                    current.id, current.function_name, formatted_args
                ).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                self.funcalls.end_current_no_write();
            }
        }

        // Update the index
        self.funcalls.last_index = Some(index);

        Ok(())
    }

    /// Public interface for ending a direct tool call - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_end_direct(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Write the tool call directly using the data stored in funcalls
        if let Some(current) = self.funcalls.get_current_funcall() {
            let formatted_args = if current.function_arguments.starts_with('{')
                && current.function_arguments.ends_with('}')
            {
                current.function_arguments.clone()
            } else {
                format!("{{{}}}", current.function_arguments)
            };
            writeln!(
                self.writer,
                r#"[{{"type":"tool_call"}},{{"id":"{}","function_name":"{}","function_arguments":"{}"}}]"#,
                current.id, current.function_name, formatted_args
            ).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

            // Reset the current function call
            self.funcalls.end_current_no_write();
        }

        Ok(())
    }
}
