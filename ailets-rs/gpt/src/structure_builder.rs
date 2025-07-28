//! A module for building structured messages in a streaming fashion.
//!
//! Collects function calls from the JSON stream and stores them in a `FunCalls` struct.

use crate::fcw_chat::FunCallsToChat;
use crate::fcw_trait::{FunCallResult, FunCallsWrite};
use crate::funcalls_builder::FunCallsBuilder;
use std::io::Write;

/// Simple wrapper to make any Write type implement FunCallsWrite
/// This is used for the DAG writer when we don't have actual DAG operations
struct WriterToFunCallsWrite<W: Write> {
    writer: W,
}

impl<W: Write> WriterToFunCallsWrite<W> {
    fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: Write> FunCallsWrite for WriterToFunCallsWrite<W> {
    fn new_item(&mut self, _id: &str, _name: &str) -> FunCallResult {
        // For now, just ignore DAG operations
        Ok(())
    }

    fn arguments_chunk(&mut self, _chunk: &str) -> FunCallResult {
        // For now, just ignore DAG operations
        Ok(())
    }

    fn end_item(&mut self) -> FunCallResult {
        // For now, just ignore DAG operations
        Ok(())
    }

    fn end(&mut self) -> FunCallResult {
        // For now, just ignore DAG operations
        Ok(())
    }
}

pub struct StructureBuilder<W1: Write, W2: Write> {
    role: Option<String>,
    message_has_content: bool,
    text_is_open: bool,
    funcalls: Option<FunCallsBuilder>,
    chat_writer: FunCallsToChat<W1>,
    dag_writer: W2,
}

impl<W1: Write, W2: Write> StructureBuilder<W1, W2> {
    #[must_use]
    pub fn new(stdout_writer: W1, dag_writer: W2) -> Self {
        StructureBuilder {
            role: None,
            message_has_content: false,
            text_is_open: false,
            funcalls: None,
            chat_writer: FunCallsToChat::new(stdout_writer),
            dag_writer,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut FunCallsToChat<W1> {
        &mut self.chat_writer
    }

    #[must_use]
    pub fn get_dag_writer(&mut self) -> &mut W2 {
        &mut self.dag_writer
    }

    #[must_use]
    pub fn get_funcalls(&self) -> Option<&FunCallsBuilder> {
        self.funcalls.as_ref()
    }

    pub fn get_funcalls_mut(&mut self) -> Option<&mut FunCallsBuilder> {
        self.funcalls.as_mut()
    }

    pub fn begin_message(&mut self) {
        self.role = None;
        self.message_has_content = false;
        self.text_is_open = false;
    }

    /// End the current message.
    /// # Errors
    /// I/O
    pub fn end_message(&mut self) -> Result<(), std::io::Error> {
        if !self.message_has_content {
            return Ok(());
        }
        if self.text_is_open {
            self.chat_writer.write_all(b"\"}]")?;
            self.text_is_open = false;
            self.chat_writer.write_all(b"\n")?;
        }

        // If there's a pending tool call in streaming mode, write it
        if let Some(funcalls) = &mut self.funcalls {
            // Create a temporary DAG writer for now - we'll fix the architecture later
            let mut temp_dag = Vec::new();
            funcalls
                .end(
                    &mut self.chat_writer,
                    &mut WriterToFunCallsWrite::new(&mut temp_dag),
                )
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        }

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
        self.chat_writer
            .write_all(b"[{\"type\":\"ctl\"},{\"role\":\"")?;
        if let Some(role) = &self.role {
            self.chat_writer.write_all(role.as_bytes())?;
        } else {
            self.chat_writer.write_all(b"assistant")?;
        }
        self.chat_writer.write_all(b"\"}]\n")?;
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
            self.chat_writer
                .write_all(b"[{\"type\":\"text\"},{\"text\":\"")?;
            self.text_is_open = true;
        }
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

        // First ensure content header is written
        if !self.message_has_content {
            self.begin_content()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        if self.text_is_open {
            self.chat_writer
                .write_all(b"\"}]")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            self.text_is_open = false;
            self.chat_writer
                .write_all(b"\n")
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }

        if let Some(funcalls) = &mut self.funcalls {
            // Create a temporary DAG writer for now - we'll fix the architecture later
            let mut temp_dag = Vec::new();
            funcalls.id(
                id,
                &mut self.chat_writer,
                &mut WriterToFunCallsWrite::new(&mut temp_dag),
            )?;
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
            // Create a temporary DAG writer for now - we'll fix the architecture later
            let mut temp_dag = Vec::new();
            funcalls.name(
                name,
                &mut self.chat_writer,
                &mut WriterToFunCallsWrite::new(&mut temp_dag),
            )?;
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
            // Create a temporary DAG writer for now - we'll fix the architecture later
            let mut temp_dag = Vec::new();
            funcalls.arguments_chunk(
                args,
                &mut self.chat_writer,
                &mut WriterToFunCallsWrite::new(&mut temp_dag),
            )?;
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
            // Create a temporary DAG writer for now - we'll fix the architecture later
            let mut temp_dag = Vec::new();
            funcalls.index(
                index,
                &mut self.chat_writer,
                &mut WriterToFunCallsWrite::new(&mut temp_dag),
            )?;
        }

        Ok(())
    }

    /// Public interface for ending a direct tool call - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_end_direct(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // End the current direct function call (only if funcalls exists)
        if let Some(funcalls) = &mut self.funcalls {
            // Create a temporary DAG writer for now - we'll fix the architecture later
            let mut temp_dag = Vec::new();
            funcalls.end_current(
                &mut self.chat_writer,
                &mut WriterToFunCallsWrite::new(&mut temp_dag),
            )?;
        }

        Ok(())
    }
}
