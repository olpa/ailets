use std::io::Write;

pub struct StructureBuilder<W: Write> {
    writer: W,
}

impl<W: Write> StructureBuilder<W> {
    pub fn new(writer: W) -> Self {
        StructureBuilder { writer }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.writer
    }

    /// # Errors
    /// I/O
    pub fn start_message(&mut self) -> std::io::Result<()> {
        self.writer.write_all(b"{")?;
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
        self.writer.write_all(br#"{"type":"text""#)?;
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
