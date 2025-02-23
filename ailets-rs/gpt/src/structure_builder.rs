use std::io::Write;

#[allow(clippy::struct_excessive_bools)]
pub struct StructureBuilder<W: Write> {
    writer: W,
    message_has_role: bool,
    message_has_content: bool,
    text_is_open: bool,
    message_is_closed: bool,
}

impl<W: Write> StructureBuilder<W> {
    #[must_use]
    pub fn new(writer: W) -> Self {
        StructureBuilder {
            writer,
            message_has_role: false,
            message_has_content: false,
            text_is_open: false,
            message_is_closed: false,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.writer
    }

    pub fn begin_message(&mut self) {
        self.message_has_role = false;
        self.message_has_content = false;
        self.text_is_open = false;
        self.message_is_closed = false;
    }

    pub fn end_message(&mut self) -> Result<(), std::io::Error> {
        if self.message_is_closed {
            return Ok(());
        }
        if !self.message_has_role && !self.message_has_content {
            return Ok(());
        }
        if !self.message_has_content {
            self.begin_content()?;
        }
        if self.text_is_open {
            self.writer.write_all(b"\"}")?;
            self.text_is_open = false;
        }
        if self.message_has_content {
            self.writer.write_all(b"]")?;
        }
        self.writer.write_all(b"}\n")?;
        self.message_is_closed = true;
        Ok(())
    }

    pub fn role(&mut self, role: &str) -> Result<(), std::io::Error> {
        if self.message_has_role {
            return Ok(());
        }
        self.writer.write_all(b"{\"role\":\"")?;
        self.writer.write_all(role.as_bytes())?;
        self.writer.write_all(b"\"")?;
        self.message_has_role = true;
        Ok(())
    }

    pub fn begin_content(&mut self) -> Result<(), std::io::Error> {
        if self.message_has_content {
            return Ok(());
        }
        if !self.message_has_role {
            self.role("assistant")?;
        }
        self.writer.write_all(b",\"content\":[")?;
        self.message_has_content = true;
        self.text_is_open = false;
        Ok(())
    }

    pub fn begin_text_chunk(&mut self) -> Result<(), std::io::Error> {
        if !self.message_has_content {
            self.begin_content()?;
        }
        if !self.text_is_open {
            self.writer.write_all(b"{\"type\":\"text\",\"text\":\"")?;
            self.text_is_open = true;
        }
        Ok(())
    }
}
