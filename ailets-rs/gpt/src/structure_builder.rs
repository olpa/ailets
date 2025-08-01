//! A module for building structured messages in a streaming fashion.
//!
//! Collects function calls from the JSON stream and stores them in a `FunCalls` struct.

use crate::fcw_chat::FunCallsToChat;
use crate::fcw_trait::{FunCallsWrite};
use crate::funcalls_builder::FunCallsBuilder;
use std::io::Write;


pub struct StructureBuilder<W1: std::io::Write, W2: FunCallsWrite> {
    funcalls: Option<FunCallsBuilder>,
    chat_writer: FunCallsToChat<W1>,
    dag_writer: W2,
}

impl<W1: std::io::Write, W2: FunCallsWrite> StructureBuilder<W1, W2> {
    #[must_use]
    pub fn new(stdout_writer: W1, dag_writer: W2) -> Self {
        StructureBuilder {
            funcalls: None,
            chat_writer: FunCallsToChat::new(stdout_writer),
            dag_writer,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut FunCallsToChat<W1> {
        &mut self.chat_writer
    }

    /// Does nothing, just a placeholder for starting a message.
    /// This is useful for maintaining a consistent interface, to pair with `end_message`.
    pub fn begin_message(&mut self) -> Result<(), std::io::Error> {
        Ok(())
    }


    /// # Errors
    /// I/O
    pub fn role(&mut self, role: &str) -> Result<(), std::io::Error> {
        self.chat_writer
            .write_all(b"[{\"type\":\"ctl\"},{\"role\":\"")?;
        self.chat_writer.write_all(role.as_bytes())?;
        self.chat_writer.write_all(b"\"}]\n")?;
        Ok(())
    }

    /// Start a text chunk
    /// # Errors
    /// I/O
    pub fn begin_text_chunk(&mut self) -> Result<(), std::io::Error> {
        self.chat_writer
            .write_all(b"[{\"type\":\"text\"},{\"text\":\"")?;
        Ok(())
    }

    /// End a text chunk
    /// # Errors
    /// I/O
    pub fn end_text_chunk(&mut self) -> Result<(), std::io::Error> {
        self.chat_writer
            .write_all(b"\"}]\n")?;
        Ok(())
    }

    /// Public interface for setting tool call ID - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_id(&mut self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure funcalls exists
        if self.funcalls.is_none() {
            self.funcalls = Some(FunCallsBuilder::new());
        }

        if let Some(funcalls) = &mut self.funcalls {
            funcalls.id(id, &mut self.chat_writer, &mut self.dag_writer)?;
        }

        Ok(())
    }

    /// Public interface for setting tool call name - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_name(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure funcalls exists
        if self.funcalls.is_none() {
            self.funcalls = Some(FunCallsBuilder::new());
        }

        if let Some(funcalls) = &mut self.funcalls {
            funcalls.name(name, &mut self.chat_writer, &mut self.dag_writer)?;
        }

        Ok(())
    }

    /// Public interface for adding tool call arguments chunk - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if I/O error occurs
    pub fn tool_call_arguments_chunk(
        &mut self,
        args: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Arguments don't need the full setup, just ensure funcalls exists
        if self.funcalls.is_none() {
            self.funcalls = Some(FunCallsBuilder::new());
        }

        if let Some(funcalls) = &mut self.funcalls {
            funcalls.arguments_chunk(args, &mut self.chat_writer, &mut self.dag_writer)?;
        }

        Ok(())
    }

    /// Public interface for setting tool call index - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_index(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure funcalls exists
        if self.funcalls.is_none() {
            self.funcalls = Some(FunCallsBuilder::new());
        }

        if let Some(funcalls) = &mut self.funcalls {
            funcalls.index(index, &mut self.chat_writer, &mut self.dag_writer)?;
        }

        Ok(())
    }

    /// Public interface for ending a direct tool call - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_end_direct(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // End the current direct function call (only if funcalls exists)
        if let Some(funcalls) = &mut self.funcalls {
            funcalls.end_current(&mut self.chat_writer, &mut self.dag_writer)?;
        }

        Ok(())
    }

    /// End the current message.
    /// # Errors
    /// I/O
    pub fn end_message(&mut self) -> Result<(), std::io::Error> {
        // If there's a pending tool call in streaming mode, write it
        if let Some(funcalls) = &mut self.funcalls {
            funcalls.end(&mut self.chat_writer, &mut self.dag_writer)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        }

        Ok(())
    }
}
