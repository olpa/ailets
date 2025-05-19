use crate::env_opts::EnvOpts;
use actor_io::AReader;
use base64::engine::general_purpose::STANDARD;
use base64::write::EncoderWriter as Base64Encoder;
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
const DEFAULT_CONTENT_TYPE: &str = "application/json";
const DEFAULT_AUTHORIZATION: &str = "Bearer {{secret}}";

pub struct StructureBuilder<W: Write> {
    writer: W,
    top: Progress,
    message: Progress,
    message_content: Progress,
    content_item: Progress,
    content_item_type: Option<String>,
    image_type: Option<String>,
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
            image_type: None,
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
        let url = self
            .env_opts
            .get("http.url")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_URL);
        self.writer.write_all(url.as_bytes())?;
        self.writer
            .write_all(b"\",\n\"method\": \"POST\",\n\"headers\": { ")?;

        // Write Content-type header
        let content_type = self
            .env_opts
            .get("http.header.Content-type")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_CONTENT_TYPE);
        self.writer.write_all(b"\"Content-type\": \"")?;
        self.writer.write_all(content_type.as_bytes())?;
        let authorization = self
            .env_opts
            .get("http.header.Authorization")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_AUTHORIZATION);
        self.writer.write_all(b"\", \"Authorization\": \"")?;
        self.writer.write_all(authorization.as_bytes())?;

        // Add remaining http.header.* parameters
        for (key, value) in &self.env_opts {
            if key.starts_with("http.header.")
                && key != "http.header.Content-type"
                && key != "http.header.Authorization"
            {
                self.writer.write_all(b", ")?;
                if let Some(header_name) = key.strip_prefix("http.header.") {
                    write!(self.writer, r#""{header_name}": "#)?;
                    serde_json::to_writer(&mut self.writer, value)?;
                }
            }
        }

        // Write the body
        self.writer.write_all(b"\" },\n\"body\": { \"model\": \"")?;
        let model = self
            .env_opts
            .get("llm.model")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_MODEL);
        self.writer.write_all(model.as_bytes())?;
        self.writer.write_all(b"\", \"stream\": ")?;
        let stream = self
            .env_opts
            .get("llm.stream")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        self.writer.write_all(stream.to_string().as_bytes())?;

        // Add remaining llm.* parameters
        for (key, value) in &self.env_opts {
            if key.starts_with("llm.") && key != "llm.model" && key != "llm.stream" {
                self.writer.write_all(b", ")?;
                if let Some(param_name) = key.strip_prefix("llm.") {
                    write!(self.writer, r#""{param_name}": "#)?;
                    serde_json::to_writer(&mut self.writer, value)?;
                }
            }
        }

        // Add messages array
        self.writer.write_all(b", \"messages\": [")?;
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
        self.image_type = None;
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
            let item_type = if item_type == "image" {
                String::from("image_url")
            } else {
                item_type
            };
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

    /// # Errors
    /// - content item is not started
    /// - I/O
    pub fn begin_image_url(&mut self) -> Result<(), String> {
        if let Progress::ChildrenAreUnexpected = self.content_item {
            return Err("Content item is not started".to_string());
        }
        self.add_item_type(String::from("image_url"))?;
        write!(self.writer, r#","image_url":{{"url":""#).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// # Errors
    /// - I/O
    pub fn end_image_url(&mut self) -> Result<(), String> {
        write!(self.writer, r#""}}"#).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// # Errors
    /// - content item is not started
    /// - I/O
    pub fn image_key(&mut self, key: &str) -> Result<(), String> {
        self.begin_image_url()?;
        write!(self.writer, "data:").map_err(|e| e.to_string())?;
        if let Some(ref image_type) = self.image_type {
            self.writer
                .write_all(image_type.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        self.writer
            .write_all(b";base64,")
            .map_err(|e| e.to_string())?;

        let cname = std::ffi::CString::new(key).map_err(|e| e.to_string())?;
        let mut blob_reader = AReader::new(&cname).map_err(|e| e.to_string())?;

        let mut encoder = Base64Encoder::new(&mut self.writer, &STANDARD);
        std::io::copy(&mut blob_reader, &mut encoder).map_err(|e| e.to_string())?;
        encoder.finish().map_err(|e| e.to_string())?;
        drop(encoder);

        self.end_image_url()
    }
}
