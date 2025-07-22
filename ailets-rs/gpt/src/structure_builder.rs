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
    pub fn output_tool_call(
        &mut self,
        tool_call: &crate::funcalls::ContentItemFunction,
    ) -> Result<(), std::io::Error> {
        if !self.message_has_content {
            self.begin_content()?;
        }
        if self.text_is_open {
            self.writer.write_all(b"\"}]")?;
            self.text_is_open = false;
        }

        self.writer
            .write_all(b"[{\"type\":\"tool_call\"},{\"id\":\"")?;
        self.writer.write_all(tool_call.id.as_bytes())?;
        self.writer.write_all(b"\",\"function_name\":\"")?;
        self.writer.write_all(tool_call.function_name.as_bytes())?;
        self.writer.write_all(b"\",\"function_arguments\":\"")?;
        self.writer
            .write_all(tool_call.function_arguments.as_bytes())?;
        self.writer.write_all(b"\"}]\n")?;
        Ok(())
    }

    /// Check if we can stream completed tool calls and output them
    /// # Errors
    /// I/O
    pub fn try_stream_completed_tool_calls(&mut self) -> Result<(), std::io::Error> {
        if let Some(tool_call) = self.funcalls.get_completed_tool_call_for_streaming() {
            self.output_tool_call(&tool_call)?;
        }
        Ok(())
    }

    /// Handle tool call index change and attempts to stream completed calls
    /// # Errors
    /// I/O  
    pub fn on_tool_call_index(&mut self, index: usize) -> Result<(), std::io::Error> {
        // Inline delta_index logic
        // Validate streaming assumption: index progression
        let validation_result = match self.funcalls.last_index {
            None => {
                // First index must be 0
                if index != 0 {
                    Err(format!("First tool call index must be 0, got {index}"))
                } else {
                    Ok(())
                }
            }
            Some(last) => {
                // Index can stay the same or increment by exactly 1, but never decrease
                if index < last {
                    Err(format!(
                        "Tool call index cannot decrease, max seen is {last}, got {index}"
                    ))
                } else if index > last + 1 {
                    Err(format!(
                        "Tool call index cannot skip values, max seen is {last}, got {index}"
                    ))
                } else {
                    // If we're moving to a new index, end the current function call
                    if index > last {
                        self.funcalls.end_current_internal();
                    }
                    Ok(())
                }
            }
        };
        
        if let Err(e) = validation_result {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e));
        }
        
        // Update last_index to track the highest seen index (enables streaming mode)
        self.funcalls.last_index = Some(index);
        self.try_stream_completed_tool_calls()
    }

    /// Handle tool call field updates and attempt streaming
    /// # Errors
    /// I/O
    pub fn on_tool_call_field_update(&mut self) -> Result<(), std::io::Error> {
        self.try_begin_streaming_current_tool_call()?;
        self.try_stream_completed_tool_calls()
    }

    /// Begin streaming output for a tool call (id and name are ready)
    /// # Errors
    /// I/O
    pub fn begin_streaming_tool_call(
        &mut self,
        tool_call: &crate::funcalls::ContentItemFunction,
    ) -> Result<(), std::io::Error> {
        if !self.message_has_content {
            self.begin_content()?;
        }
        if self.text_is_open {
            self.writer.write_all(b"\"}]")?;
            self.text_is_open = false;
        }

        self.writer
            .write_all(b"[{\"type\":\"tool_call\"},{\"id\":\"")?;
        self.writer.write_all(tool_call.id.as_bytes())?;
        self.writer.write_all(b"\",\"function_name\":\"")?;
        self.writer.write_all(tool_call.function_name.as_bytes())?;
        self.writer.write_all(b"\",\"function_arguments\":\"")?;

        Ok(())
    }

    /// Stream arguments chunk using `write_long_bytes`
    /// # Errors
    /// I/O
    pub fn stream_tool_call_arguments_chunk(
        &mut self,
        rjiter: &mut scan_json::RJiter,
    ) -> Result<(), std::io::Error> {
        if !self.funcalls.is_streaming_arguments() {
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
        if self.funcalls.is_streaming_arguments() {
            self.writer.write_all(b"\"}]\n")?;
            self.funcalls.close_current_streaming_tool_call();
        }
        Ok(())
    }

    /// Check if we should start streaming a tool call (id and name available)
    /// # Errors
    /// I/O
    pub fn try_begin_streaming_current_tool_call(&mut self) -> Result<(), std::io::Error> {
        if let Some(tool_call) = self.funcalls.get_current_tool_call_for_streaming() {
            self.begin_streaming_tool_call(&tool_call)?;
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
}
