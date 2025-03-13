use std::io::Write;

#[derive(Debug)]
pub enum Progress {
    ChildrenAreUnexpected,
    WaitingForFirstChild,
    WriteIsStarted, // to have idempotent "really_start" and to close the element
    ChildIsWritten, // to write the comma
}

fn is_write_started(progress: Progress) -> bool {
    progress == Progress::WriteIsStarted || progress == Progress::ChildIsWritten
}

pub struct StructureBuilder<W: Write> {
    writer: W,
    top: Progress,
    message: Progress,
    message_content: Progress,
}

impl<W: Write> StructureBuilder<W> {
    pub fn new(writer: W) -> Self {
        StructureBuilder {
            writer,
            top: Progress::WaitingForFirstChild,
            message: Progress::ChildrenAreUnexpected,
            message_content: Progress::ChildrenAreUnexpected,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.writer
    }

    /// # Errors
    /// I/O
    pub fn start_message(&mut self) -> std::io::Result<()> {
        self.message = Progress::WaitingForFirstChild;
        self.message_content = Progress::ChildrenAreUnexpected;
        Ok(())
    }

    fn really_start_message(&mut self) -> std::Result<(), String> {
        if self.message == Progress::ChildrenAreUnexpected {
            return Err("Message is not started".to_string());
        }
        if is_write_started(self.message) {
            return Ok();
        }
        if self.top == Progress::ChildIsWritten {
            self.writer.write_all(b",")?;
        }
        self.writer.write_all(b"{")?;
        self.message = Progress::WriteIsStarted;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn end_message(&mut self) -> std::io::Result<()> {
        if is_write_started(self.message) {
            self.writer.write_all(b"}")?;
            self.top = Progress::ChildIsWritten;
        }
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn add_role(&mut self, role: &str) -> std::io::Result<()> {
        self.really_start_message()?;
        write!(self.writer, r#""role":"{role}""#)?;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn start_content(&mut self) -> std::io::Result<()> {
        self.writer.write_all(br#","content":["#)?;
        Ok(())
    }

    pub fn start_content_item(&mut self) -> std::io::Result<()> {
        if self.should_write_div_content_item {
            self.writer.write_all(b",")?;
        }
        self.should_write_div_content_item = true;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn start_text_item(&mut self) -> std::io::Result<()> {
        if self.should_write_div_content_item {
            self.writer.write_all(b",")?;
        }
        self.writer.write_all(br#"{"type":"text""#)?;

        self.should_write_div_content_item = true;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn add_text(&mut self, text: &str) -> std::io::Result<()> {
        write!(self.writer, r#","text":"{text}""#)?;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn end_text_item(&mut self) -> std::io::Result<()> {
        self.writer.write_all(b"}")?;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn end_content(&mut self) -> std::io::Result<()> {
        self.writer.write_all(b"]")?;
        Ok(())
    }
}
