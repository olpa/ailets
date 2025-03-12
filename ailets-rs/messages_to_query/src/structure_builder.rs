use std::io::Write;

pub struct StructureBuilder<W: Write> {
    writer: W,
    has_message: bool,
    has_content_item: bool,
}

impl<W: Write> StructureBuilder<W> {
    pub fn new(writer: W) -> Self {
        StructureBuilder {
            writer,
            has_message: false,
            has_content_item: false,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.writer
    }

    /// # Errors
    /// I/O
    pub fn start_message(&mut self) -> std::io::Result<()> {
        if self.has_message {
            self.writer.write_all(b",")?;
        }
        self.writer.write_all(b"{")?;

        self.has_message = true;
        self.has_content_item = false;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn add_role(&mut self, role: &str) -> std::io::Result<()> {
        write!(self.writer, r#""role":"{role}""#)?;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn start_content(&mut self) -> std::io::Result<()> {
        self.writer.write_all(br#","content":["#)?;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn start_text_item(&mut self) -> std::io::Result<()> {
        if self.has_content_item {
            self.writer.write_all(b",")?;
        }
        self.writer.write_all(br#"{"type":"text""#)?;

        self.has_content_item = true;
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

    /// # Errors
    /// I/O
    pub fn end_message(&mut self) -> std::io::Result<()> {
        self.writer.write_all(b"}")?;
        Ok(())
    }
}
