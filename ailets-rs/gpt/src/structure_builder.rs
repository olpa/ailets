//! A module for building structured messages in a streaming fashion.
//!
//! Collects function calls from the JSON stream and stores them in a `FunCalls` struct.

use crate::dagops::DagOpsTrait;
use crate::fcw_chat::FunCallsToChat;
use crate::fcw_dag::FunCallsToDag;
use crate::fcw_trait::FunCallsWrite;
use crate::funcalls_builder::FunCallsBuilder;
use std::io::Write;

pub struct ArgumentsChunkWriter<'a, W1: Write, D: DagOpsTrait> {
    builder: &'a mut StructureBuilder<W1, D>,
}

impl<'a, W1: Write, D: DagOpsTrait> ArgumentsChunkWriter<'a, W1, D> {
    fn new(builder: &'a mut StructureBuilder<W1, D>) -> Self {
        Self { builder }
    }
}

impl<'a, W1: Write + 'static, D: DagOpsTrait> Write for ArgumentsChunkWriter<'a, W1, D> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = std::str::from_utf8(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        self.builder
            ._tool_call_arguments_chunk(s)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct StructureBuilder<W1: std::io::Write, D: DagOpsTrait> {
    funcalls: FunCallsBuilder<D>,
    chat_writer: FunCallsToChat<W1>,
    dag_writer: FunCallsToDag,
    text_is_open: bool,
    is_tool_section_open: bool,
}

impl<W1: std::io::Write + 'static, D: DagOpsTrait> StructureBuilder<W1, D> {
    #[must_use]
    pub fn new(stdout_writer: W1, dag_writer: FunCallsToDag, dagops: D) -> Self {
        StructureBuilder {
            funcalls: FunCallsBuilder::new(dagops),
            chat_writer: FunCallsToChat::new(stdout_writer),
            dag_writer,
            text_is_open: false,
            is_tool_section_open: false,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut FunCallsToChat<W1> {
        &mut self.chat_writer
    }

    #[must_use]
    pub fn get_arguments_chunk_writer(&mut self) -> ArgumentsChunkWriter<'_, W1, D> {
        ArgumentsChunkWriter::new(self)
    }

    /// Auto-close text if it's open
    fn auto_close_text_if_open(&mut self) -> Result<(), std::io::Error> {
        if self.text_is_open {
            self.chat_writer.write_all(b"\"}]\n")?;
            self.text_is_open = false;
        }
        Ok(())
    }

    /// Auto-close tool section if it's open
    fn auto_close_tool_section_if_open(&mut self) -> Result<(), std::io::Error> {
        if self.is_tool_section_open {
            self.funcalls
                .end(&mut self.chat_writer, &mut self.dag_writer)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            self.is_tool_section_open = false;
        }
        Ok(())
    }

    /// Does nothing, just a placeholder for starting a message.
    /// This is useful for maintaining a consistent interface, to pair with `end_message`.
    ///
    /// # Errors
    /// Returns an error if auto-closing text or tool operations fail.
    pub fn begin_message(&mut self) -> Result<(), std::io::Error> {
        self.auto_close_text_if_open()?;
        self.auto_close_tool_section_if_open()?;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn role(&mut self, role: &str) -> Result<(), std::io::Error> {
        self.auto_close_text_if_open()?;
        self.auto_close_tool_section_if_open()?;
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
        self.auto_close_tool_section_if_open()?;
        if !self.text_is_open {
            self.chat_writer
                .write_all(b"[{\"type\":\"text\"},{\"text\":\"")?;
            self.text_is_open = true;
        }
        Ok(())
    }

    /// End a text chunk
    /// # Errors
    /// I/O
    pub fn end_text_chunk(&mut self) -> Result<(), std::io::Error> {
        self.chat_writer.write_all(b"\"}]\n")?;
        self.text_is_open = false;
        Ok(())
    }

    /// Public interface for setting tool call ID - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_id(&mut self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.auto_close_text_if_open()?;
        self.funcalls.id(id, &mut self.chat_writer, &mut self.dag_writer)?;
        self.is_tool_section_open = true;
        Ok(())
    }

    /// Public interface for setting tool call name - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_name(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.auto_close_text_if_open()?;
        self.funcalls.name(name, &mut self.chat_writer, &mut self.dag_writer)?;
        self.is_tool_section_open = true;
        Ok(())
    }

    /// Private interface for adding tool call arguments chunk - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if I/O error occurs
    fn _tool_call_arguments_chunk(&mut self, args: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.auto_close_text_if_open()?;
        self.funcalls.arguments_chunk(
            args.as_bytes(),
            &mut self.chat_writer,
            &mut self.dag_writer,
        )?;
        self.is_tool_section_open = true;
        Ok(())
    }

    /// Public interface for setting tool call index - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_index(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        self.auto_close_text_if_open()?;
        self.funcalls.index(index, &mut self.chat_writer, &mut self.dag_writer)?;
        self.is_tool_section_open = true;
        Ok(())
    }

    /// Public interface for ending a direct tool call - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_end_if_direct(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.funcalls.end_item_if_direct(&mut self.chat_writer, &mut self.dag_writer)?;
        // Don't modify is_tool_section_open - this is not a section-level operation
        Ok(())
    }

    /// End the current message.
    /// # Errors
    /// I/O
    pub fn end_message(&mut self) -> Result<(), std::io::Error> {
        self.auto_close_text_if_open()?;
        self.auto_close_tool_section_if_open()?;

        Ok(())
    }

    /// End processing and finalize all writers.
    /// # Errors
    /// I/O or other errors from the underlying writers
    pub fn end(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.chat_writer.end()?;
        self.dag_writer.end()?;
        Ok(())
    }
}
