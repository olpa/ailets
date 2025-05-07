use crate::env_opts::EnvOpts;
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

const DEFAULT_URL: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MODEL: &str = "gpt-4o-mini";

pub struct StructureBuilder<W: Write> {
    writer: W,
    top: Progress,
    message: Progress,
    message_content: Progress,
    content_item: Progress,
    content_item_type: Option<String>,
    env_opts: EnvOpts,
}

impl<W: Write> StructureBuilder<W> {
    pub fn new(writer: W, env_opts: EnvOpts) -> Self {
        StructureBuilder {
            writer,
            top: Progress::WaitingForFirstChild,
            message: Progress::ChildrenAreUnexpected,
            message_content: Progress::ChildrenAreUnexpected,
            content_item: Progress::ChildrenAreUnexpected,
            content_item_type: None,
            env_opts,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.writer
    }

    fn really_begin(&mut self) -> Result<(), std::io::Error> {
        if is_write_started(&self.top) {
            return Ok(());
        }
        self.writer.write_all(b"{ \"url\": \"")?;
        self.writer.write_all(DEFAULT_URL.as_bytes())?;
        self.writer
            .write_all(b"\",\n\"method\": \"POST\",\n\"headers\": { ")?;
        self.writer.write_all(b"\"Content-type\": \"application/json\", \"Authorization\": \"Bearer {{secret('openai','gpt4o')}}\" },")?;
        self.writer.write_all(b"\n\"body\": { \"model\": \"")?;
        self.writer.write_all(DEFAULT_MODEL.as_bytes())?;
        self.writer
            .write_all(b"\", \"stream\": true, \"messages\": [")?;
        self.top = Progress::WriteIsStarted;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn end(&mut self) -> Result<(), String> {
        if let Progress::ChildIsWritten = self.top {
            self.writer.write_all(b"]}}\n").map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn begin_message(&mut self) -> Result<(), String> {
        self.message = Progress::WaitingForFirstChild;
        self.message_content = Progress::ChildrenAreUnexpected;
        self.content_item = Progress::ChildrenAreUnexpected;
        Ok(())
    }

    /// # Errors
    /// - message is not started
    /// - I/O
    fn really_begin_message(&mut self) -> Result<(), String> {
        if let Progress::ChildrenAreUnexpected = self.message {
            return Err("Message is not started".to_string());
        }
        if is_write_started(&self.message) {
            return Ok(());
        }
        self.really_begin().map_err(|e| e.to_string())?;
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
            if self.content_item_type.is_none() {
                // closed enforce "content" key, even if there is no content
                self.begin_content()?;
                self.end_content()?;
            }
            self.writer.write_all(b"}").map_err(|e| e.to_string())?;
            self.top = Progress::ChildIsWritten;
        }
        self.message_content = Progress::ChildrenAreUnexpected;
        self.content_item = Progress::ChildrenAreUnexpected;
        Ok(())
    }

    /// # Errors
    /// - message is not started
    /// - I/O
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
    /// - message is not started
    /// - I/O
    pub fn begin_content(&mut self) -> Result<(), String> {
        self.message_content = Progress::WaitingForFirstChild;
        self.content_item = Progress::ChildrenAreUnexpected;
        self.content_item_type = None;
        // Unlike for other containers, allow empty content
        self.really_begin_content()?;
        Ok(())
    }

    /// # Errors
    /// - message is not started
    /// - content is not started
    /// - I/O
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
            .write_all(b"\"content\":[\n")
            .map_err(|e| e.to_string())?;
        self.message_content = Progress::WriteIsStarted;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn end_content(&mut self) -> Result<(), String> {
        if is_write_started(&self.message_content) {
            self.writer.write_all(b"\n]").map_err(|e| e.to_string())?;
            self.message = Progress::ChildIsWritten;
        }
        self.content_item = Progress::ChildrenAreUnexpected;
        if self.content_item_type.is_none() {
            // Signal that "content" key is present
            self.content_item_type = Some(String::new());
        }
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn begin_content_item(&mut self) -> Result<(), String> {
        self.content_item = Progress::WaitingForFirstChild;
        self.content_item_type = None;
        Ok(())
    }

    /// # Errors
    /// - content is not started
    /// - content item is not started
    /// - I/O
    fn really_begin_content_item(&mut self) -> Result<(), String> {
        if let Progress::ChildrenAreUnexpected = self.content_item {
            return Err("Content item is not started".to_string());
        }
        if is_write_started(&self.content_item) {
            return Ok(());
        }
        self.really_begin_content()?;
        if let Progress::ChildIsWritten = self.message_content {
            self.writer.write_all(b",\n").map_err(|e| e.to_string())?;
        }
        self.writer.write_all(b"{").map_err(|e| e.to_string())?;
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

    /// # Errors
    /// - content item is not started
    /// - I/O
    pub fn add_item_type(&mut self, item_type: String) -> Result<(), String> {
        self.really_begin_content_item()?;
        if let Some(ref existing_type) = self.content_item_type {
            if existing_type != &item_type {
                return Err(format!(
                    "Wrong content item type: already typed as \"{existing_type}\", new type is \"{item_type}\""
                ));
            }
        } else {
            write!(self.writer, r#""type":"{item_type}""#).map_err(|e| e.to_string())?;
            self.content_item_type = Some(item_type);
        }
        Ok(())
    }

    /// # Errors
    /// - content item is not started
    /// - I/O
    pub fn begin_text(&mut self) -> Result<(), String> {
        if let Progress::ChildrenAreUnexpected = self.content_item {
            return Err("Content item is not started".to_string());
        }
        self.add_item_type(String::from("text"))?;
        write!(self.writer, r#","text":""#).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// # Errors
    /// - I/O
    pub fn end_text(&mut self) -> Result<(), String> {
        write!(self.writer, "\"").map_err(|e| e.to_string())?;
        Ok(())
    }
}
