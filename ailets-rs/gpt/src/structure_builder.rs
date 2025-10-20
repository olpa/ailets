//! A module for building structured messages in a streaming fashion.

use crate::dagops::DagOpsTrait;
use crate::funcalls_builder::FunCallsBuilder;

pub struct ArgumentsChunkWriter<'a, W: embedded_io::Write, D: DagOpsTrait> {
    builder: &'a mut StructureBuilder<W, D>,
}

impl<'a, W: embedded_io::Write, D: DagOpsTrait> ArgumentsChunkWriter<'a, W, D> {
    fn new(builder: &'a mut StructureBuilder<W, D>) -> Self {
        Self { builder }
    }
}

impl<W: embedded_io::Write, D: DagOpsTrait> embedded_io::ErrorType
    for ArgumentsChunkWriter<'_, W, D>
{
    type Error = embedded_io::ErrorKind;
}

impl<W: embedded_io::Write, D: DagOpsTrait> embedded_io::Write for ArgumentsChunkWriter<'_, W, D> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let s = std::str::from_utf8(buf).map_err(|_| embedded_io::ErrorKind::InvalidData)?;
        self.builder
            .tool_call_arguments_chunk(s)
            .map_err(|_| embedded_io::ErrorKind::Other)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

// Normal chat content goes to the stdout,
// function calls inject workflows to the DAG using `FunCallsBuilder`
pub struct StructureBuilder<W: embedded_io::Write, D: DagOpsTrait> {
    funcalls: FunCallsBuilder<D>,
    stdout: W,
    text_is_open: bool,
    role: Option<String>,
    text_section_started: bool,
    /// Optional extended error message to provide more details than the static StreamOp::Error
    last_error: Option<String>,
}

impl<W: embedded_io::Write, D: DagOpsTrait> StructureBuilder<W, D> {
    #[must_use]
    pub fn new(stdout_writer: W, dagops: D) -> Self {
        StructureBuilder {
            funcalls: FunCallsBuilder::new(dagops),
            stdout: stdout_writer,
            text_is_open: false,
            role: None,
            text_section_started: false,
            last_error: None,
        }
    }

    /// Sets the last error message for later retrieval
    pub fn set_error(&mut self, error: String) {
        self.last_error = Some(error);
    }

    /// Takes the last error message, clearing it
    pub fn take_error(&mut self) -> Option<String> {
        self.last_error.take()
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.stdout
    }

    #[must_use]
    pub fn get_arguments_chunk_writer(&mut self) -> ArgumentsChunkWriter<'_, W, D> {
        ArgumentsChunkWriter::new(self)
    }

    /// Auto-close text if it's open
    fn auto_close_text_if_open(&mut self) -> Result<(), W::Error> {
        if self.text_is_open {
            self.stdout.write_all(b"\"}]\n")?;
            self.text_is_open = false;
        }
        Ok(())
    }

    /// Does nothing, just a placeholder for starting a message.
    /// This is useful for maintaining a consistent interface, to pair with `end_message`.
    ///
    /// # Errors
    /// Returns an error if auto-closing text operations fail.
    pub fn begin_message(&mut self) -> Result<(), W::Error> {
        self.auto_close_text_if_open()?;
        // Clear any role from previous message and reset text section flag
        self.role = None;
        self.text_section_started = false;
        Ok(())
    }

    /// Set the role for the current message,
    /// the header will be written by the first item in the message.
    /// # Errors
    /// I/O
    pub fn role(&mut self, role: &str) -> Result<(), W::Error> {
        // Check if this is a different role than the current effective role
        let is_different_role = match &self.role {
            Some(current) => current != role,
            None => role != "assistant", // Default is "assistant"
        };

        if is_different_role {
            // Auto-close text if open and reset text section for new role
            self.auto_close_text_if_open()?;
            self.text_section_started = false;
        }

        // Store the role (update current role)
        self.role = Some(role.to_string());
        Ok(())
    }

    /// Start a text chunk
    /// # Errors
    /// I/O
    pub fn begin_text_chunk(&mut self) -> Result<(), W::Error> {
        // Write role header if we haven't started a text section yet
        if !self.text_section_started {
            // Set role to "assistant" if not set
            if self.role.is_none() {
                self.role = Some("assistant".to_string());
            }

            if let Some(ref role) = self.role {
                self.stdout.write_all(b"[{\"type\":\"ctl\"},{\"role\":\"")?;
                self.stdout.write_all(role.as_bytes())?;
                self.stdout.write_all(b"\"}]\n")?;
            }

            self.text_section_started = true;
        }

        if !self.text_is_open {
            self.stdout
                .write_all(b"[{\"type\":\"text\"},{\"text\":\"")?;
            self.text_is_open = true;
        }
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn end_text_chunk(&mut self) -> Result<(), W::Error> {
        self.stdout.write_all(b"\"}]\n")?;
        self.text_is_open = false;
        Ok(())
    }

    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_id(&mut self, id: &str) -> Result<(), String> {
        self.funcalls.id(id)?;
        Ok(())
    }

    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_name(&mut self, name: &str) -> Result<(), String> {
        self.funcalls.name(name)?;
        Ok(())
    }

    /// Private interface for adding tool call arguments chunk
    /// This is used internally by the `ArgumentsChunkWriter`.
    /// # Errors
    /// Returns error if I/O error occurs
    fn tool_call_arguments_chunk(&mut self, args: &str) -> Result<(), String> {
        self.funcalls.arguments_chunk(args.as_bytes())?;
        Ok(())
    }

    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_index(&mut self, index: usize) -> Result<(), String> {
        self.funcalls.index(index)?;
        Ok(())
    }

    /// # Errors
    /// Returns error if validation fails or I/O error occurs
    pub fn tool_call_end_if_direct(&mut self) -> Result<(), String> {
        self.funcalls.end_item_if_direct()?;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn end_message(&mut self) -> Result<(), W::Error> {
        self.auto_close_text_if_open()?;
        Ok(())
    }

    /// # Errors
    /// I/O or other errors from the underlying writers
    pub fn end(&mut self) -> Result<(), String> {
        self.funcalls.end()?;
        Ok(())
    }
}
