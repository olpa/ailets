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

        // If there's a pending tool call in streaming mode, write it
        if self.funcalls.last_index.is_some() {
            // Funcall streaming functionality has been removed
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

    /// Public interface for setting tool call ID - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_id(&mut self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Create FunCallsToChat writer and forward to funcalls
        let mut chat_writer = FunCallsToChat::new(&mut self.writer);
        self.funcalls.id(id, &mut chat_writer)?;

        Ok(())
    }

    /// Public interface for setting tool call name - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_name(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Create FunCallsToChat writer and forward to funcalls
        let mut chat_writer = FunCallsToChat::new(&mut self.writer);
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
        // Create FunCallsToChat writer and forward to funcalls
        let mut chat_writer = FunCallsToChat::new(&mut self.writer);
        self.funcalls.arguments_chunk(args, &mut chat_writer)?;

        Ok(())
    }

    /// Public interface for setting tool call index - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_index(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        // Check if we need to finalize the previous tool call when index increases
        if let Some(last_index) = self.funcalls.last_index {
            if index > last_index {
                // Write the previous tool call before starting the new one
                // Funcall streaming functionality has been removed
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
        // Ensure content header is written
        if !self.message_has_content {
            self.begin_content()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        if self.text_is_open {
            self.writer
                .write_all(b"\"}]")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            self.text_is_open = false;
            self.writer
                .write_all(b"\n")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }

        // Funcall streaming functionality has been removed

        Ok(())
    }
}
