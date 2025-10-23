use crate::action_error::ActionError;
use embedded_io::Write;

pub struct StructureBuilder<W: Write> {
    writer: W,
    need_para_divider: bool,
    /// Optional extended error message to provide more details than the static `StreamOp::Error`
    last_error: Option<ActionError>,
}

impl<W: Write> StructureBuilder<W> {
    pub fn new(writer: W) -> Self {
        StructureBuilder {
            writer,
            need_para_divider: false,
            last_error: None,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.writer
    }

    pub fn start_paragraph(&mut self) -> Result<(), embedded_io::ErrorKind> {
        if self.need_para_divider {
            self.writer
                .write_all(b"\n\n")
                .map_err(|_| embedded_io::ErrorKind::Other)?;
        }
        self.need_para_divider = true;
        Ok(())
    }

    pub fn finish_with_newline(&mut self) -> Result<(), embedded_io::ErrorKind> {
        if self.need_para_divider {
            self.writer
                .write_all(b"\n")
                .map_err(|_| embedded_io::ErrorKind::Other)?;
        }
        self.need_para_divider = false;
        Ok(())
    }

    /// Store a detailed error that occurred during action handling
    pub fn set_error(&mut self, error: ActionError) {
        self.last_error = Some(error);
    }

    /// Take the stored error, leaving None in its place
    pub fn take_error(&mut self) -> Option<ActionError> {
        self.last_error.take()
    }
}
