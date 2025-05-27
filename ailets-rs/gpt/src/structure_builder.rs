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
        }
        self.writer.write_all(b"]}\n")?;
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
        if let Some(role) = &self.role {
            self.writer.write_all(b"{\"role\":\"")?;
            self.writer.write_all(role.as_bytes())?;
            self.writer.write_all(b"\",\"content\":[")?;
        } else {
            self.writer
                .write_all(b"{\"role\":\"assistant\",\"content\":[")?;
        }
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
}
