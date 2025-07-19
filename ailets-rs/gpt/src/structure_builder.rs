//! A module for building structured messages in a streaming fashion.
//!
//! Collects function calls from the JSON stream and stores them in a `FunCalls` struct.

use crate::funcalls::FunCalls;
use std::io::Write;

pub struct StructureBuilder<W: Write> {
    writer: W,
    role: Option<String>,
    message_has_content: bool,
    text_is_open: bool,
    message_is_closed: bool,
    funcalls: FunCalls,
    streaming_mode: bool,
    last_streamed_index: Option<usize>,
    tool_call_open: bool,
    tool_call_arguments_open: bool,
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
            streaming_mode: false,
            last_streamed_index: None,
            tool_call_open: false,
            tool_call_arguments_open: false,
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
        self.streaming_mode = false;
        self.last_streamed_index = None;
        self.tool_call_open = false;
        self.tool_call_arguments_open = false;
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

    /// Output a single tool call in streaming fashion
    /// # Errors
    /// I/O
    pub fn output_tool_call(&mut self, tool_call: &crate::funcalls::ContentItemFunction) -> Result<(), std::io::Error> {
        if !self.message_has_content {
            self.begin_content()?;
        }
        if self.text_is_open {
            self.writer.write_all(b"\"}]")?;
            self.text_is_open = false;
        }
        
        self.writer.write_all(b"[{\"type\":\"tool_call\"},{\"id\":\"")?;
        self.writer.write_all(tool_call.id.as_bytes())?;
        self.writer.write_all(b"\",\"function_name\":\"")?;
        self.writer.write_all(tool_call.function_name.as_bytes())?;
        self.writer.write_all(b"\",\"function_arguments\":\"")?;
        self.writer.write_all(tool_call.function_arguments.as_bytes())?;
        self.writer.write_all(b"\"}]\n")?;
        Ok(())
    }

    /// Process and output all tool calls from funcalls
    /// # Errors
    /// I/O
    pub fn inject_tool_calls(&mut self) -> Result<(), std::io::Error> {
        let tool_calls = self.funcalls.get_tool_calls().clone();
        for tool_call in &tool_calls {
            self.output_tool_call(tool_call)?;
        }
        Ok(())
    }

    /// Enable streaming mode when tool call deltas are detected
    pub fn enable_streaming_mode(&mut self) {
        self.streaming_mode = true;
    }

    /// Check if we can stream completed tool calls and output them
    /// # Errors
    /// I/O
    pub fn try_stream_completed_tool_calls(&mut self) -> Result<(), std::io::Error> {
        if !self.streaming_mode {
            return Ok(());
        }

        let tool_calls = self.funcalls.get_tool_calls().clone();
        let start_index = self.last_streamed_index.map_or(0, |i| i + 1);
        
        // Check for completed tool calls that can be streamed
        for (index, tool_call) in tool_calls.iter().enumerate().skip(start_index) {
            // A tool call is complete if all required fields are present
            // Arguments can be empty ("") but id and function_name must be non-empty
            if !tool_call.id.is_empty() 
                && !tool_call.function_name.is_empty() {
                // For streaming, we consider it complete when we have a basic structure
                // Arguments might still be streaming in, but we can output when they're done
                // This means arguments being non-empty OR having actual content
                if !tool_call.function_arguments.is_empty() {
                    self.output_tool_call(tool_call)?;
                    self.last_streamed_index = Some(index);
                }
            } else {
                // Stop at first incomplete tool call (streaming order)
                break;
            }
        }
        Ok(())
    }

    /// Handle tool call index change - enables streaming and attempts to stream completed calls
    /// # Errors
    /// I/O  
    pub fn on_tool_call_index(&mut self, index: usize) -> Result<(), std::io::Error> {
        self.enable_streaming_mode();
        self.funcalls.delta_index(index);
        self.try_stream_completed_tool_calls()
    }

    /// Handle tool call field updates and attempt streaming
    /// # Errors
    /// I/O
    pub fn on_tool_call_field_update(&mut self) -> Result<(), std::io::Error> {
        if self.streaming_mode {
            self.try_begin_streaming_current_tool_call()?;
            self.try_stream_completed_tool_calls()
        } else {
            Ok(())
        }
    }

    /// Begin streaming output for a tool call (id and name are ready)
    /// # Errors
    /// I/O
    pub fn begin_streaming_tool_call(&mut self, tool_call: &crate::funcalls::ContentItemFunction) -> Result<(), std::io::Error> {
        if !self.message_has_content {
            self.begin_content()?;
        }
        if self.text_is_open {
            self.writer.write_all(b"\"}]")?;
            self.text_is_open = false;
        }
        
        self.writer.write_all(b"[{\"type\":\"tool_call\"},{\"id\":\"")?;
        self.writer.write_all(tool_call.id.as_bytes())?;
        self.writer.write_all(b"\",\"function_name\":\"")?;
        self.writer.write_all(tool_call.function_name.as_bytes())?;
        self.writer.write_all(b"\",\"function_arguments\":\"")?;
        
        self.tool_call_open = true;
        self.tool_call_arguments_open = true;
        Ok(())
    }

    /// Stream arguments chunk using write_long_bytes
    /// # Errors
    /// I/O
    pub fn stream_tool_call_arguments_chunk(&mut self, rjiter: &mut scan_json::RJiter) -> Result<(), std::io::Error> {
        if !self.tool_call_arguments_open {
            return Ok(());
        }
        
        // Stream the arguments directly to the output
        if let Err(e) = rjiter.write_long_bytes(&mut self.writer) {
            return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
        }
        Ok(())
    }

    /// Close the streaming tool call
    /// # Errors
    /// I/O  
    pub fn close_streaming_tool_call(&mut self) -> Result<(), std::io::Error> {
        if self.tool_call_open {
            if self.tool_call_arguments_open {
                self.writer.write_all(b"\"}]\n")?;
                self.tool_call_arguments_open = false;
            }
            self.tool_call_open = false;
        }
        Ok(())
    }

    /// Check if we should start streaming a tool call (id and name available)
    /// # Errors
    /// I/O
    pub fn try_begin_streaming_current_tool_call(&mut self) -> Result<(), std::io::Error> {
        if !self.streaming_mode || self.tool_call_open {
            return Ok(());
        }

        // Get current tool call data
        if let Some(current_funcall) = self.funcalls.get_current_funcall().clone() {
            if !current_funcall.id.is_empty() && !current_funcall.function_name.is_empty() {
                self.begin_streaming_tool_call(&current_funcall)?;
            }
        }
        Ok(())
    }
}
