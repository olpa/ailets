use crate::env_opts::EnvOpts;
use actor_io::AReader;
use actor_runtime::annotate_error;
use base64::engine::general_purpose::STANDARD;
use base64::write::EncoderWriter as Base64Encoder;
use linked_hash_map::LinkedHashMap;
use serde::Serialize;
use std::io;
use std::io::Write;

struct StrFormatter {}

impl serde_json::ser::Formatter for StrFormatter {
    fn begin_string<W>(&mut self, _writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        Ok(())
    }
    fn end_string<W>(&mut self, _writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum Divider {
    Prologue,
    MessageComma,
    ItemNone,
    ItemComma,
}

#[derive(Debug, PartialEq)]
pub enum ItemAttrMode {
    RaiseError,
    Collect,
    Passthrough,
    Drop,
}

const DEFAULT_URL: &str = "https://api.openai.com/v1/chat/completions";
const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_CONTENT_TYPE: &str = "application/json";
const DEFAULT_AUTHORIZATION: &str = "Bearer {{secret}}";

pub struct StructureBuilder<W: Write> {
    writer: W,
    env_opts: EnvOpts,
    divider: Divider,
    item_attr: Option<LinkedHashMap<String, String>>,
    item_attr_mode: ItemAttrMode,
}

impl<W: Write> StructureBuilder<W> {
    pub fn new(writer: W, env_opts: EnvOpts) -> Self {
        StructureBuilder {
            writer,
            env_opts,
            divider: Divider::Prologue,
            item_attr: None,
            item_attr_mode: ItemAttrMode::RaiseError,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Close JSON object, unless nothing was written yet
    ///
    /// There is no "pub fn begin" because it is done implicitly by the call chain
    /// `pub fn handle_role` -> `begin_message` -> `write_prologue`
    ///
    /// # Errors
    /// I/O
    pub fn end(&mut self) -> Result<(), String> {
        if self.divider != Divider::Prologue {
            self.end_message()?;
            self.writer.write_all(b"]}}\n").map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn write_prologue(&mut self) -> Result<(), String> {
        self.writer
            .write_all(b"{ \"url\": \"")
            .map_err(|e| e.to_string())?;
        let url = self
            .env_opts
            .get("http.url")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_URL);
        self.writer
            .write_all(url.as_bytes())
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(b"\",\n\"method\": \"POST\",\n\"headers\": { ")
            .map_err(|e| e.to_string())?;

        // Write Content-type header
        let content_type = self
            .env_opts
            .get("http.header.Content-type")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_CONTENT_TYPE);
        self.writer
            .write_all(b"\"Content-type\": \"")
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(content_type.as_bytes())
            .map_err(|e| e.to_string())?;
        let authorization = self
            .env_opts
            .get("http.header.Authorization")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_AUTHORIZATION);
        self.writer
            .write_all(b"\", \"Authorization\": \"")
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(authorization.as_bytes())
            .map_err(|e| e.to_string())?;

        // Add remaining http.header.* parameters
        for (key, value) in &self.env_opts {
            if key.starts_with("http.header.")
                && key != "http.header.Content-type"
                && key != "http.header.Authorization"
            {
                self.writer.write_all(b", ").map_err(|e| e.to_string())?;
                if let Some(header_name) = key.strip_prefix("http.header.") {
                    write!(self.writer, r#""{header_name}": "#).map_err(|e| e.to_string())?;
                    serde_json::to_writer(&mut self.writer, value).map_err(|e| e.to_string())?;
                }
            }
        }

        // Write the body
        self.writer
            .write_all(b"\" },\n\"body\": { \"model\": \"")
            .map_err(|e| e.to_string())?;
        let model = self
            .env_opts
            .get("llm.model")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_MODEL);
        self.writer
            .write_all(model.as_bytes())
            .map_err(|e| e.to_string())?;
        self.writer
            .write_all(b"\", \"stream\": ")
            .map_err(|e| e.to_string())?;
        let stream = self
            .env_opts
            .get("llm.stream")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        self.writer
            .write_all(stream.to_string().as_bytes())
            .map_err(|e| e.to_string())?;

        // Add remaining llm.* parameters
        for (key, value) in &self.env_opts {
            if key.starts_with("llm.") && key != "llm.model" && key != "llm.stream" {
                self.writer.write_all(b", ").map_err(|e| e.to_string())?;
                if let Some(param_name) = key.strip_prefix("llm.") {
                    write!(self.writer, r#""{param_name}": "#).map_err(|e| e.to_string())?;
                    serde_json::to_writer(&mut self.writer, value).map_err(|e| e.to_string())?;
                }
            }
        }

        // Add messages array
        self.writer
            .write_all(b", \"messages\": [")
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Called by `handle_role`
    /// # Errors
    /// I/O
    fn begin_message(&mut self, role: &str) -> Result<(), String> {
        if self.divider == Divider::Prologue {
            self.write_prologue()?;
        }
        self.end_message()?; // Can update the divider
        if self.divider == Divider::MessageComma {
            self.writer.write_all(b",").map_err(|e| e.to_string())?;
        }

        write!(self.writer, r#"{{"role":"{role}","content":["#).map_err(|e| e.to_string())?;
        self.writer.write_all(b"\n").map_err(|e| e.to_string())?;
        self.divider = Divider::ItemNone;
        self.item_attr_mode = ItemAttrMode::RaiseError;
        Ok(())
    }

    /// Called by `end`, by `begin_message` or indirectly by `handle_role` through `begin_message`
    /// # Errors
    /// I/O
    fn end_message(&mut self) -> Result<(), String> {
        if self.divider == Divider::ItemComma || self.divider == Divider::ItemNone {
            self.writer.write_all(b"\n]}").map_err(|e| e.to_string())?;
            self.divider = Divider::MessageComma;
        }
        self.item_attr_mode = ItemAttrMode::RaiseError;
        Ok(())
    }

    /// Reset the internal state for a new content item
    /// # Errors
    /// I/O
    pub fn begin_item(&mut self) -> Result<(), String> {
        self.item_attr_mode = ItemAttrMode::Collect;
        self.item_attr = None;
        Ok(())
    }

    /// Start output JSON object for a content item
    /// # Errors
    /// - I/O
    /// - missing "type" attribute
    fn really_begin_item(&mut self) -> Result<(), String> {
        match self.item_attr {
            None => {
                return Err("Missing 'type' attribute".to_string());
            }
            Some(ref attrs) => {
                match attrs.get("type") {
                    None => {
                        return Err("Missing 'type' attribute".to_string());
                    }
                    Some(item_type) => {
                        let item_type = if item_type == "image" {
                            "image_url"
                        } else {
                            item_type
                        };
                        // `divider` is set to `ItemComma` by `end_item`
                        if self.divider == Divider::ItemComma {
                            self.writer.write_all(b",\n").map_err(|e| e.to_string())?;
                        }
                        write!(self.writer, r#"{{"type":"#).map_err(|e| e.to_string())?;
                        serde_json::to_writer(&mut self.writer, item_type)
                            .map_err(|e| e.to_string())?;

                        for (key, value) in attrs {
                            if key == "type" {
                                continue;
                            }
                            if item_type == "image_url"
                                && (key == "detail" || key == "content_type")
                            {
                                continue;
                            }
                            write!(self.writer, r#","{key}":"#).map_err(|e| e.to_string())?;
                            serde_json::to_writer(&mut self.writer, value)
                                .map_err(|e| e.to_string())?;
                        }

                        self.item_attr_mode = ItemAttrMode::Passthrough;
                    }
                }
            }
        }
        Ok(())
    }

    /// Close JSON object for a content item, unless it is a control item which was not written
    /// # Errors
    /// I/O
    pub fn end_item(&mut self) -> Result<(), String> {
        let is_ctl = self
            .item_attr
            .as_ref()
            .and_then(|attrs| attrs.get("type"))
            .map_or(true, |t| t == "ctl");
        if !is_ctl {
            self.writer.write_all(b"}").map_err(|e| e.to_string())?;
            self.divider = Divider::ItemComma;
        }

        self.item_attr = None;
        self.item_attr_mode = ItemAttrMode::RaiseError;
        Ok(())
    }

    //
    // Action-triggering item content
    //

    /// # Errors
    /// - content item is not started
    /// - I/O
    pub fn handle_role(&mut self, role: &str) -> Result<(), String> {
        if let ItemAttrMode::RaiseError = self.item_attr_mode {
            return Err("Content item is not started".to_string());
        }
        let item_type = self
            .item_attr
            .as_ref()
            .ok_or_else(|| "Content item attributes are not set".to_string())?
            .get("type")
            .ok_or_else(|| "Content item type is not set".to_string())?;

        if item_type != "ctl" {
            return Err(format!("Expected type 'ctl', got '{item_type}'"));
        }

        self.begin_message(role)
    }

    /// # Errors
    /// - content item is not started
    /// - for "type" attribute, the value is unknown or conflicting with the already typed item
    /// - I/O
    pub fn add_item_attribute(&mut self, key: String, value: String) -> Result<(), String> {
        if let ItemAttrMode::RaiseError = self.item_attr_mode {
            return Err("Content item is not started".to_string());
        }

        if self.item_attr_mode == ItemAttrMode::Passthrough {
            write!(self.writer, r#","{key}":"#).map_err(|e| e.to_string())?;
            serde_json::to_writer(&mut self.writer, &value).map_err(|e| e.to_string())?;
            return Ok(());
        }

        if self.item_attr.is_none() {
            self.item_attr = Some(LinkedHashMap::new());
        }
        if let Some(ref mut attrs) = self.item_attr {
            if key == "type" {
                if let Some(existing_type) = attrs.get("type") {
                    if existing_type != &value {
                        return Err(format!("Wrong content item type: already typed as \"{existing_type}\", new type is \"{value}\""));
                    }
                    return Ok(());
                }
                if !matches!(value.as_str(), "text" | "image" | "ctl") {
                    return Err(format!(
                        "Invalid type value: '{value}'. Allowed values are: text, image, ctl"
                    ));
                }
            }
            attrs.insert(key, value);
        }
        Ok(())
    }

    /// # Errors
    /// - content item is not started
    /// - I/O
    pub fn begin_text(&mut self) -> Result<(), String> {
        if let ItemAttrMode::RaiseError = self.item_attr_mode {
            return Err("Content item is not started".to_string());
        }
        self.add_item_attribute(String::from("type"), String::from("text"))?;
        self.really_begin_item()?;

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
        if let ItemAttrMode::RaiseError = self.item_attr_mode {
            return Err("Content item is not started".to_string());
        }
        self.add_item_attribute(String::from("type"), String::from("image"))?;
        self.really_begin_item()?;

        write!(self.writer, r#","image_url":{{"#).map_err(|e| e.to_string())?;
        if let Some(ref attrs) = self.item_attr {
            if let Some(ref detail) = attrs.get("detail") {
                write!(self.writer, r#""detail":"#).map_err(|e| e.to_string())?;
                serde_json::to_writer(&mut self.writer, detail).map_err(|e| e.to_string())?;
                write!(self.writer, r#","#).map_err(|e| e.to_string())?;
            }
        }
        write!(self.writer, r#""url":""#).map_err(|e| e.to_string())?;
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
        let err_to_str = |e: std::io::Error| {
            let dyn_err: Box<dyn std::error::Error> = e.into();
            let annotated_error = annotate_error(dyn_err, format!("image key `{key}`").as_str());
            annotated_error.to_string()
        };
        self.begin_image_url()?;
        write!(self.writer, "data:").map_err(err_to_str)?;
        if let Some(ref attrs) = self.item_attr {
            if let Some(ref content_type) = attrs.get("content_type") {
                let mut ser =
                    serde_json::ser::Serializer::with_formatter(&mut self.writer, StrFormatter {});
                content_type
                    .serialize(&mut ser)
                    .map_err(|e| e.to_string())?;
            }
        }
        self.writer.write_all(b";base64,").map_err(err_to_str)?;

        let cname = std::ffi::CString::new(key).map_err(|e| e.to_string())?;
        let mut blob_reader = AReader::new(&cname).map_err(err_to_str)?;

        let mut encoder = Base64Encoder::new(&mut self.writer, &STANDARD);
        std::io::copy(&mut blob_reader, &mut encoder).map_err(err_to_str)?;
        encoder.finish().map_err(err_to_str)?;
        drop(encoder);

        self.end_image_url()
    }
}
