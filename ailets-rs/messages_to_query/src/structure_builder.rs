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

//
// The state machine, to generate on the levels:
// top -> messages -> message with role -> content -> content item
// top -> messages -> message with role -> tool_calls -> function call item
// top -> tools -> toolspec item
//
#[derive(Debug, PartialEq)]
pub enum Divider {
    Prologue,     // Need to write the prologue, then start "messages" or "tools"
    MessageComma, // Add `,` after the last "messages" or "tools"
    ItemNone,     // First item in message, "content", or "tool_calls"
    ItemCommaContent,
    ItemCommaFunctions,
    ItemCommaToolspecs,
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

    // ----------------------------------------------------
    // State machine: End a level
    //

    //
    // General logic: for the path
    //
    //   up_level1 -> up_level2 -> ... -> foo -> down_level1 -> down_level2 -> ...
    //
    // handle all the states:
    //
    // - not in the path: raise error
    // - up levels: raise error
    // - down levels:
    //   - end the level "down_level1", which may recursively end other down levels
    //   - continue like the level "foo"
    // - foo: end the level "foo"
    //

    // Never call `end_item` to avoid infinite recursion.
    // Let it be the responsibility of the client.

    /// # Errors
    /// I/O
    pub fn end(&mut self) -> Result<(), String> {
        match self.divider {
            Divider::Prologue => return Ok(()),
            Divider::ItemNone | Divider::ItemCommaContent | Divider::ItemCommaFunctions => {
                self.end_messages()?;
            }
            Divider::MessageComma => {
                // Messages are already closed, nothing to do
            }
            Divider::ItemCommaToolspecs => {
                self.end_toolspecs()?;
            }
        }
        self.writer.write_all(b"}}\n").map_err(|e| e.to_string())?;
        self.divider = Divider::Prologue;
        Ok(())
    }

    fn end_toolspecs(&mut self) -> Result<(), String> {
        match self.divider {
            Divider::Prologue
            | Divider::MessageComma
            | Divider::ItemNone
            | Divider::ItemCommaContent
            | Divider::ItemCommaFunctions => {
                return Err(format!(
                    "Internal error: Wrong state {:?} to end tools",
                    self.divider
                ))
            }
            Divider::ItemCommaToolspecs => {
                // not: self.end_item()?;
            }
        }
        self.writer.write_all(b"] ").map_err(|e| e.to_string())?;
        self.divider = Divider::MessageComma;
        Ok(())
    }

    fn end_messages(&mut self) -> Result<(), String> {
        match self.divider {
            Divider::Prologue | Divider::ItemCommaToolspecs => {
                return Err(format!(
                    "Internal error: Cannot end messages while in tools section: {:?}",
                    self.divider
                ))
            }
            Divider::ItemCommaContent | Divider::ItemNone | Divider::ItemCommaFunctions => {
                self.end_message_content()?;
            }
            Divider::MessageComma => {}
        }
        self.writer.write_all(b"]").map_err(|e| e.to_string())?;
        self.divider = Divider::MessageComma;
        Ok(())
    }

    fn end_message_content(&mut self) -> Result<(), String> {
        match self.divider {
            Divider::Prologue | Divider::MessageComma | Divider::ItemCommaToolspecs => {
                return Err(format!(
                    "Internal error: Wrong state {:?} to end message content",
                    self.divider
                ))
            }
            Divider::ItemCommaContent => {
                // not: self.end_item()?;
                // Close the content array
                self.writer.write_all(b"\n]").map_err(|e| e.to_string())?;
            }
            Divider::ItemCommaFunctions => {
                // not: self.end_item()?;
                // Close the tool_calls array
                self.writer.write_all(b"\n]").map_err(|e| e.to_string())?;
            }
            Divider::ItemNone => {}
        }
        self.writer.write_all(b"}").map_err(|e| e.to_string())?;
        self.divider = Divider::Prologue; // Bad state, to be updated in `end_messages`
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn end_item(&mut self) -> Result<(), String> {
        match self.divider {
            Divider::Prologue | Divider::MessageComma => Err(format!(
                "Internal error: Wrong state {:?} to end item",
                self.divider
            )),
            Divider::ItemNone => Ok(()), // Nothing to end when no item has been started
            Divider::ItemCommaContent
            | Divider::ItemCommaFunctions
            | Divider::ItemCommaToolspecs => self.end_item_logic(),
        }
    }

    // ----------------------------------------------------
    // State machine: Begin a level
    //

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

        Ok(())
    }

    /// # Errors
    /// I/O
    fn begin_message(&mut self, role: &str) -> Result<(), String> {
        self.want_messages()?;

        let need_comma = match self.divider {
            Divider::ItemCommaContent | Divider::ItemNone | Divider::ItemCommaFunctions => {
                self.end_message_content()?;
                self.divider = Divider::MessageComma;
                true
            }
            Divider::MessageComma => false,
            Divider::Prologue | Divider::ItemCommaToolspecs => {
                return Err(format!(
                    "Invalid state for begin_message: {:?}",
                    self.divider
                ))
            }
        };

        if need_comma {
            self.writer.write_all(b",").map_err(|e| e.to_string())?;
        }

        write!(self.writer, r#"{{"role":"{role}""#).map_err(|e| e.to_string())?;
        self.divider = Divider::ItemNone;
        self.item_attr_mode = ItemAttrMode::RaiseError;
        Ok(())
    }

    /// # Errors
    /// I/O
    pub fn begin_item(&mut self) -> Result<(), String> {
        self.item_attr_mode = ItemAttrMode::Collect;
        self.item_attr = None;
        Ok(())
    }

    /// # Errors
    /// I/O, state machine errors
    fn want_messages(&mut self) -> Result<(), String> {
        match self.divider {
            Divider::Prologue => {
                self.write_prologue()?;
                self.writer
                    .write_all(b", \"messages\": [")
                    .map_err(|e| e.to_string())?;
                self.divider = Divider::MessageComma;
            }
            Divider::ItemCommaToolspecs => {
                // Close tools section and start messages
                self.writer.write_all(b"]").map_err(|e| e.to_string())?;
                self.writer
                    .write_all(b", \"messages\": [")
                    .map_err(|e| e.to_string())?;
                self.divider = Divider::MessageComma;
            }
            Divider::MessageComma
            | Divider::ItemNone
            | Divider::ItemCommaContent
            | Divider::ItemCommaFunctions => {}
        }
        Ok(())
    }

    /// # Errors
    /// I/O, state machine errors
    fn want_toolspecs(&mut self) -> Result<(), String> {
        match self.divider {
            Divider::ItemCommaToolspecs => {
                self.writer.write_all(b",\n").map_err(|e| e.to_string())?;
                return Ok(());
            }
            Divider::MessageComma
            | Divider::ItemNone
            | Divider::ItemCommaContent
            | Divider::ItemCommaFunctions => {
                self.end_messages()?;
            }
            Divider::Prologue => {
                self.write_prologue()?;
            }
        }
        self.writer
            .write_all(b",\n\"tools\": [")
            .map_err(|e| e.to_string())?;
        self.divider = Divider::ItemCommaToolspecs;
        Ok(())
    }

    /// # Errors
    /// I/O, state machine errors
    fn want_message_item(&mut self, is_function: bool) -> Result<(), String> {
        match (&self.divider, is_function) {
            // Same section, just add comma
            (Divider::ItemCommaContent, false) | (Divider::ItemCommaFunctions, true) => {
                self.writer.write_all(b",\n").map_err(|e| e.to_string())?;
            }
            // The very beginning, write message content/function section
            // Assume that the attribute `role` has been written already, therefore add a comma
            (Divider::ItemNone, false) => {
                self.writer
                    .write_all(b",\n\"content\":[\n")
                    .map_err(|e| e.to_string())?;
                self.divider = Divider::ItemCommaContent;
            }
            (Divider::ItemNone, true) => {
                self.writer
                    .write_all(b",\n\"tool_calls\":[\n")
                    .map_err(|e| e.to_string())?;
                self.divider = Divider::ItemCommaFunctions;
            }
            // Switch between "content" to "tool_calls"
            (Divider::ItemCommaContent, true) | (Divider::ItemCommaFunctions, false) => {
                self.writer.write_all(b"]").map_err(|e| e.to_string())?;
                self.divider = Divider::ItemNone;
                self.want_message_item(is_function)?;
            }
            // Should start message first
            (Divider::ItemCommaToolspecs, _)
            | (Divider::MessageComma, _)
            | (Divider::Prologue, _) => {
                self.begin_message("user")?;
                self.divider = Divider::ItemNone;
                self.want_message_item(is_function)?;
            }
        }
        Ok(())
    }

    // ----------------------------------------------------
    // Begin an item, end an item
    //

    /// # Errors
    /// - I/O
    /// - missing "type" attribute
    fn begin_item_logic(&mut self) -> Result<(), String> {
        // Extract needed values to avoid borrowing conflicts
        let item_type = self
            .item_attr
            .as_ref()
            .ok_or_else(|| "Missing 'type' attribute".to_string())?
            .get("type")
            .ok_or_else(|| "Missing 'type' attribute".to_string())?
            .clone();

        // Need `write_item_type` to avoid collision on `function` for `tool_calls` and `toolspec`
        let is_toolspec = item_type == "toolspec";
        let write_item_type = if is_toolspec {
            "function"
        } else if item_type == "image" {
            "image_url"
        } else {
            &item_type
        };

        // Begin section for content/function items
        if is_toolspec {
            self.want_toolspecs()?;
        } else {
            let is_function = item_type == "function";
            self.want_message_item(is_function)?;
        }

        let attrs = self
            .item_attr
            .as_ref()
            .ok_or_else(|| "Missing 'type' attribute".to_string())?;

        // Write the item
        write!(self.writer, r#"{{"type":"#).map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut self.writer, write_item_type).map_err(|e| e.to_string())?;

        for (key, value) in attrs {
            if key == "type" {
                continue;
            }
            if item_type == "image" && (key == "detail" || key == "content_type") {
                continue;
            }
            write!(self.writer, r#","{key}":"#).map_err(|e| e.to_string())?;
            serde_json::to_writer(&mut self.writer, value).map_err(|e| e.to_string())?;
        }

        self.item_attr_mode = ItemAttrMode::Passthrough;
        Ok(())
    }

    fn end_item_logic(&mut self) -> Result<(), String> {
        let is_ctl = self
            .item_attr
            .as_ref()
            .and_then(|attrs| attrs.get("type"))
            .map_or(true, |t| t == "ctl");

        if !is_ctl {
            if self.item_attr_mode == ItemAttrMode::Collect {
                self.begin_item_logic()?;
            }
            self.writer.write_all(b"}").map_err(|e| e.to_string())?;
        }

        self.item_attr = None;
        self.item_attr_mode = ItemAttrMode::RaiseError;
        Ok(())
    }

    // ----------------------------------------------------
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
            return Err(format!(
                "For 'role' attribute, expected item type 'ctl', got '{item_type}'"
            ));
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
                if !matches!(
                    value.as_str(),
                    "text" | "image" | "ctl" | "function" | "toolspec"
                ) {
                    return Err(format!(
                        "Invalid type value: '{value}'. Allowed values are: text, image, function, ctl, toolspec"
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
        self.begin_item_logic()?;

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
        self.begin_item_logic()?;

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

    /// # Errors
    /// - content item is not started
    /// - I/O
    pub fn begin_function_arguments(&mut self) -> Result<(), String> {
        if let ItemAttrMode::RaiseError = self.item_attr_mode {
            return Err("Content item is not started".to_string());
        }
        let name = self
            .item_attr
            .as_mut()
            .ok_or_else(|| "Content item attributes not found".to_string())?
            .remove("name")
            .ok_or_else(|| "Missing required 'name' attribute for 'type=function'".to_string())?;

        self.add_item_attribute(String::from("type"), String::from("function"))?;
        self.begin_item_logic()?;

        write!(self.writer, r#","function":{{"name":"#).map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut self.writer, &name).map_err(|e| e.to_string())?;
        write!(self.writer, r#","arguments":""#).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// # Errors
    /// - I/O
    pub fn end_function_arguments(&mut self) -> Result<(), String> {
        self.writer.write_all(b"\"}").map_err(|e| e.to_string())?;
        Ok(())
    }

    /// # Errors
    /// - content item is not started
    /// - I/O
    /// - file not found
    /// - invalid JSON in file
    pub fn toolspec_key(&mut self, key: &str) -> Result<(), String> {
        let err_to_str = |e: std::io::Error| {
            let dyn_err: Box<dyn std::error::Error> = e.into();
            let annotated_error = annotate_error(dyn_err, format!("toolspec key `{key}`").as_str());
            annotated_error.to_string()
        };
        let cname = std::ffi::CString::new(key).map_err(|e| e.to_string())?;
        let mut blob_reader = AReader::new(&cname).map_err(err_to_str)?;
        let mut buffer = [0u8; 1024];
        let rjiter = scan_json::RJiter::new(&mut blob_reader, &mut buffer);
        let rjiter_cell = std::cell::RefCell::new(rjiter);
        
        self.toolspec_rjiter_with_key(&rjiter_cell, Some(key))
    }

    /// # Errors
    /// - content item is not started
    /// - I/O
    /// - invalid JSON in rjiter
    pub fn toolspec_rjiter(&mut self, rjiter_cell: &std::cell::RefCell<scan_json::RJiter>) -> Result<(), String> {
        self.toolspec_rjiter_with_key(rjiter_cell, None)
    }

    /// # Errors
    /// - content item is not started
    /// - I/O
    /// - invalid JSON in rjiter
    fn toolspec_rjiter_with_key(&mut self, rjiter_cell: &std::cell::RefCell<scan_json::RJiter>, key: Option<&str>) -> Result<(), String> {
        if let ItemAttrMode::RaiseError = self.item_attr_mode {
            return Err("Content item is not started".to_string());
        }
        self.add_item_attribute(String::from("type"), String::from("toolspec"))?;
        self.begin_item_logic()?;

        write!(self.writer, r#","function":"#).map_err(|e| e.to_string())?;
        
        let writer = self.get_writer();

        let error_prefix = if let Some(k) = key {
            format!("toolspec key `{k}`")
        } else {
            "toolspec rjiter".to_string()
        };

        scan_json::idtransform::idtransform(rjiter_cell, writer)
            .map_err(|e| format!("{error_prefix}: {e}"))?;

        Ok(())
    }
}
