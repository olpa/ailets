use std::io::Write;

#[derive(Debug)]
pub enum Progress {
    ChildrenAreUnexpected,
    WaitingForFirstChild,
    WriteIsStarted, // to have idempotent "really_start" and to close the element
    ChildIsWritten, // to write the comma
}

fn is_write_started(progress: &Progress) -> bool {
    matches!(
        progress,
        Progress::WriteIsStarted | Progress::ChildIsWritten
    )
}

pub struct StructureBuilder<W: Write> {
    writer: W,
    top: Progress,
    message: Progress,
    message_content: Progress,
    content_item: Progress,
}

impl<W: Write> StructureBuilder<W> {
    pub fn new(writer: W) -> Self {
        StructureBuilder {
            writer,
            top: Progress::WaitingForFirstChild,
            message: Progress::ChildrenAreUnexpected,
            message_content: Progress::ChildrenAreUnexpected,
            content_item: Progress::ChildrenAreUnexpected,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.writer
    }

    /// # Errors
    /// I/O
    pub fn begin_message(&mut self) -> Result<(), String> {
        self.message = Progress::WaitingForFirstChild;
        self.message_content = Progress::ChildrenAreUnexpected;
        self.content_item = Progress::ChildrenAreUnexpected;
        Ok(())
    }

    fn really_begin_message(&mut self) -> Result<(), String> {
        if let Progress::ChildrenAreUnexpected = self.message {
            return Err("Message is not started".to_string());
        }
        if is_write_started(&self.message) {
            return Ok(());
        }
        if let Progress::ChildIsWritten = self.top {
            self.writer.write_all(b",").map_err(|e| e.to_string())?;
        }
        self.writer.write_all(b"{").map_err(|e| e.to_string())?;
        self.message = Progress::WriteIsStarted;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn end_message(&mut self) -> Result<(), String> {
        if is_write_started(&self.message) {
            self.writer.write_all(b"}").map_err(|e| e.to_string())?;
            self.top = Progress::ChildIsWritten;
        }
        self.message_content = Progress::ChildrenAreUnexpected;
        self.content_item = Progress::ChildrenAreUnexpected;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn add_role(&mut self, role: &str) -> Result<(), String> {
        self.really_begin_message()?;
        if let Progress::ChildIsWritten = self.message {
            self.writer.write_all(b",").map_err(|e| e.to_string())?;
        }
        write!(self.writer, r#""role":"{role}""#).map_err(|e| e.to_string())?;
        self.message = Progress::ChildIsWritten;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn begin_content(&mut self) -> Result<(), String> {
        self.message_content = Progress::WaitingForFirstChild;
        self.content_item = Progress::ChildrenAreUnexpected;
        Ok(())
    }

    fn really_begin_content(&mut self) -> Result<(), String> {
        if let Progress::ChildrenAreUnexpected = self.message_content {
            return Err("Content is not started".to_string());
        }
        if is_write_started(&self.message_content) {
            return Ok(());
        }
        self.really_begin_message()?;
        if let Progress::ChildIsWritten = self.message {
            self.writer.write_all(b",").map_err(|e| e.to_string())?;
        }
        self.writer
            .write_all(br#""content":["#)
            .map_err(|e| e.to_string())?;
        self.message_content = Progress::WriteIsStarted;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn end_content(&mut self) -> Result<(), String> {
        if is_write_started(&self.message_content) {
            self.writer.write_all(b"]").map_err(|e| e.to_string())?;
            self.message = Progress::ChildIsWritten;
        }
        self.content_item = Progress::ChildrenAreUnexpected;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn begin_content_item(&mut self) -> Result<(), String> {
        self.content_item = Progress::WaitingForFirstChild;
        Ok(())
    }

    fn really_begin_content_item(&mut self) -> Result<(), String> {
        if let Progress::ChildrenAreUnexpected = self.content_item {
            return Err("Content item is not started".to_string());
        }
        if is_write_started(&self.content_item) {
            return Ok(());
        }
        self.really_begin_content()?;
        if let Progress::ChildIsWritten = self.message_content {
            self.writer.write_all(b",").map_err(|e| e.to_string())?;
        }
        self.content_item = Progress::WriteIsStarted;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn end_content_item(&mut self) -> Result<(), String> {
        if is_write_started(&self.content_item) {
            self.writer.write_all(b"}").map_err(|e| e.to_string())?;
            self.message_content = Progress::ChildIsWritten;
        }
        Ok(())
    }

    fn write_item_prologue(&mut self, item_type: &str) -> Result<(), String> {
        self.writer
            .write_all(br#"{"type":""#)
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(item_type.as_bytes())
            .map_err(|e| e.to_string())?;
        self.writer.write_all(b"\",\"").map_err(|e| e.to_string())?;
        self.writer
            .write_all(item_type.as_bytes())
            .map_err(|e| e.to_string())?;
        self.writer.write_all(b"\":").map_err(|e| e.to_string())?;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn add_text(&mut self, text: &str) -> Result<(), String> {
        if let Progress::ChildrenAreUnexpected = self.content_item {
            return Err("Content item is not started".to_string());
        }
        self.really_begin_content_item()?;
        self.write_item_prologue("text")?;
        self.writer.write_all(b"\"").map_err(|e| e.to_string())?;
        self.writer
            .write_all(text.as_bytes())
            .map_err(|e| e.to_string())?;
        self.writer.write_all(b"\"").map_err(|e| e.to_string())?;
        self.content_item = Progress::ChildIsWritten;
        Ok(())
    }
}
