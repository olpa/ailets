//! A module for building structured messages in a streaming fashion.
//!
//! Collects function calls from the JSON stream and stores them in a `FunCalls` struct.

use crate::fcw_chat::FunCallsToChat;
use crate::fcw_trait::{FunCallsWrite};
use crate::funcalls_builder::FunCallsBuilder;
use std::io::Write;

pub struct ArgumentsChunkWriter<'a, W1: Write, W2: FunCallsWrite> {
    builder: &'a mut StructureBuilder<W1, W2>,
}

impl<'a, W1: Write, W2: FunCallsWrite> ArgumentsChunkWriter<'a, W1, W2> {
    fn new(builder: &'a mut StructureBuilder<W1, W2>) -> Self {
        Self { builder }
    }
}

impl<'a, W1: Write, W2: FunCallsWrite> Write for ArgumentsChunkWriter<'a, W1, W2> {
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

    #[must_use]
    pub fn get_arguments_chunk_writer(&mut self) -> ArgumentsChunkWriter<W1, W2> {
        ArgumentsChunkWriter::new(self)
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
        match &mut self.funcalls {
            Some(funcalls) => {
                funcalls.id(id, &mut self.chat_writer, &mut self.dag_writer)?;
            }
            None => {
                self.funcalls = Some(FunCallsBuilder::new());
                if let Some(funcalls) = &mut self.funcalls {
                    funcalls.id(id, &mut self.chat_writer, &mut self.dag_writer)?;
                }
            }
        }
        Ok(())
    }

    /// Public interface for setting tool call name - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_name(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        match &mut self.funcalls {
            Some(funcalls) => {
                funcalls.name(name, &mut self.chat_writer, &mut self.dag_writer)?;
            }
            None => return Err("tool_call_name called without initializing funcalls".into())
        }
        Ok(())
    }

    /// Private interface for adding tool call arguments chunk - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if I/O error occurs
    fn _tool_call_arguments_chunk(
        &mut self,
        args: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match &mut self.funcalls {
            Some(funcalls) => {
                funcalls.arguments_chunk(args.as_bytes(), &mut self.chat_writer, &mut self.dag_writer)?;
            }
            None => return Err("tool_call_arguments_chunk called without initializing funcalls".into())
        }
        Ok(())
    }

    /// Public interface for setting tool call index - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_index(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
        match &mut self.funcalls {
            Some(funcalls) => {
                funcalls.index(index, &mut self.chat_writer, &mut self.dag_writer)?;
            }
            None => {
                self.funcalls = Some(FunCallsBuilder::new());
                if let Some(funcalls) = &mut self.funcalls {
                    funcalls.index(index, &mut self.chat_writer, &mut self.dag_writer)?;
                }
            }
        }
        Ok(())
    }

    /// Public interface for ending a direct tool call - forwards to funcalls and handles streaming
    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_end_direct(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        match &mut self.funcalls {
            Some(funcalls) => {
                funcalls.end_current(&mut self.chat_writer, &mut self.dag_writer)?;
            }
            None => return Err("tool_call_end_direct called without initializing funcalls".into())
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
