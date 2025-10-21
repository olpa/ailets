use crate::action_error::ActionError;
use crate::env_opts::EnvOpts;
use actor_io::AReader;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use linked_hash_map::LinkedHashMap;

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

pub struct StructureBuilder<W: embedded_io::Write> {
    writer: W,
    env_opts: EnvOpts,
    divider: Divider,
    item_attr: Option<LinkedHashMap<String, String>>,
    item_attr_mode: ItemAttrMode,
    /// Optional extended error message to provide more details than the static `StreamOp::Error`
    last_error: Option<ActionError>,
}

impl<W: embedded_io::Write> StructureBuilder<W> {
    pub fn new(writer: W, env_opts: EnvOpts) -> Self {
        StructureBuilder {
            writer,
            env_opts,
            divider: Divider::Prologue,
            item_attr: None,
            item_attr_mode: ItemAttrMode::RaiseError,
            last_error: None,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Store a detailed error that occurred during action handling
    pub fn set_error(&mut self, error: ActionError) {
        self.last_error = Some(error);
    }

    /// Take the stored error, leaving None in its place
    pub fn take_error(&mut self) -> Option<ActionError> {
        self.last_error.take()
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
    /// I/O, state machine errors
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
        embedded_io::Write::write_all(&mut self.writer, b"}}\n").map_err(|e| format!("{e:?}"))?;
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
        embedded_io::Write::write_all(&mut self.writer, b"] ").map_err(|e| format!("{e:?}"))?;
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
        embedded_io::Write::write_all(&mut self.writer, b"]").map_err(|e| format!("{e:?}"))?;
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
                embedded_io::Write::write_all(&mut self.writer, b"\n]")
                    .map_err(|e| format!("{e:?}"))?;
            }
            Divider::ItemCommaFunctions => {
                // not: self.end_item()?;
                // Close the tool_calls array
                embedded_io::Write::write_all(&mut self.writer, b"\n]")
                    .map_err(|e| format!("{e:?}"))?;
            }
            Divider::ItemNone => {}
        }
        embedded_io::Write::write_all(&mut self.writer, b"}").map_err(|e| format!("{e:?}"))?;
        self.divider = Divider::Prologue; // Bad state, to be updated in `end_messages`
        Ok(())
    }

    /// # Errors
    /// I/O, state machine errors
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
        embedded_io::Write::write_all(&mut self.writer, b"{ \"url\": \"")
            .map_err(|e| format!("{e:?}"))?;
        let url = self
            .env_opts
            .get("http.url")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_URL);
        embedded_io::Write::write_all(&mut self.writer, url.as_bytes())
            .map_err(|e| format!("{e:?}"))?;
        embedded_io::Write::write_all(
            &mut self.writer,
            b"\",\n\"method\": \"POST\",\n\"headers\": { ",
        )
        .map_err(|e| format!("{e:?}"))?;

        // Write Content-type header
        let content_type = self
            .env_opts
            .get("http.header.Content-type")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_CONTENT_TYPE);
        embedded_io::Write::write_all(&mut self.writer, b"\"Content-type\": \"")
            .map_err(|e| format!("{e:?}"))?;
        embedded_io::Write::write_all(&mut self.writer, content_type.as_bytes())
            .map_err(|e| format!("{e:?}"))?;
        let authorization = self
            .env_opts
            .get("http.header.Authorization")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_AUTHORIZATION);
        embedded_io::Write::write_all(&mut self.writer, b"\", \"Authorization\": \"")
            .map_err(|e| format!("{e:?}"))?;
        embedded_io::Write::write_all(&mut self.writer, authorization.as_bytes())
            .map_err(|e| format!("{e:?}"))?;

        // Add remaining http.header.* parameters
        for (key, value) in &self.env_opts {
            if key.starts_with("http.header.")
                && key != "http.header.Content-type"
                && key != "http.header.Authorization"
            {
                embedded_io::Write::write_all(&mut self.writer, b", ")
                    .map_err(|e| format!("{e:?}"))?;
                if let Some(header_name) = key.strip_prefix("http.header.") {
                    let header_part = format!(r#""{header_name}": "#);
                    embedded_io::Write::write_all(&mut self.writer, header_part.as_bytes())
                        .map_err(|e| format!("{e:?}"))?;
                    let value_json = serde_json::to_string(value).map_err(|e| format!("{e:?}"))?;
                    embedded_io::Write::write_all(&mut self.writer, value_json.as_bytes())
                        .map_err(|e| format!("{e:?}"))?;
                }
            }
        }

        // Write the body
        embedded_io::Write::write_all(&mut self.writer, b"\" },\n\"body\": { \"model\": \"")
            .map_err(|e| format!("{e:?}"))?;
        let model = self
            .env_opts
            .get("llm.model")
            .and_then(|v| v.as_str())
            .unwrap_or(DEFAULT_MODEL);
        embedded_io::Write::write_all(&mut self.writer, model.as_bytes())
            .map_err(|e| format!("{e:?}"))?;
        embedded_io::Write::write_all(&mut self.writer, b"\", \"stream\": ")
            .map_err(|e| format!("{e:?}"))?;
        let stream = self
            .env_opts
            .get("llm.stream")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        embedded_io::Write::write_all(&mut self.writer, stream.to_string().as_bytes())
            .map_err(|e| format!("{e:?}"))?;

        // Add remaining llm.* parameters
        for (key, value) in &self.env_opts {
            if key.starts_with("llm.") && key != "llm.model" && key != "llm.stream" {
                embedded_io::Write::write_all(&mut self.writer, b", ")
                    .map_err(|e| format!("{e:?}"))?;
                if let Some(param_name) = key.strip_prefix("llm.") {
                    let param_part = format!(r#""{param_name}": "#);
                    embedded_io::Write::write_all(&mut self.writer, param_part.as_bytes())
                        .map_err(|e| format!("{e:?}"))?;
                    let value_json = serde_json::to_string(value).map_err(|e| format!("{e:?}"))?;
                    embedded_io::Write::write_all(&mut self.writer, value_json.as_bytes())
                        .map_err(|e| format!("{e:?}"))?;
                }
            }
        }

        Ok(())
    }

    /// # Errors
    /// I/O, state machine errors
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
            embedded_io::Write::write_all(&mut self.writer, b",").map_err(|e| format!("{e:?}"))?;
        }

        let role_str = format!(r#"{{"role":"{role}""#);
        embedded_io::Write::write_all(&mut self.writer, role_str.as_bytes())
            .map_err(|e| format!("{e:?}"))?;
        self.divider = Divider::ItemNone;
        self.item_attr_mode = ItemAttrMode::RaiseError;
        Ok(())
    }

    /// # Errors
    /// I/O, state machine errors
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
                embedded_io::Write::write_all(&mut self.writer, b", \"messages\": [")
                    .map_err(|e| format!("{e:?}"))?;
                self.divider = Divider::MessageComma;
            }
            Divider::ItemCommaToolspecs => {
                // Close tools section and start messages
                embedded_io::Write::write_all(&mut self.writer, b"]")
                    .map_err(|e| format!("{e:?}"))?;
                embedded_io::Write::write_all(&mut self.writer, b", \"messages\": [")
                    .map_err(|e| format!("{e:?}"))?;
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
                embedded_io::Write::write_all(&mut self.writer, b",\n")
                    .map_err(|e| format!("{e:?}"))?;
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
        embedded_io::Write::write_all(&mut self.writer, b",\n\"tools\": [")
            .map_err(|e| format!("{e:?}"))?;
        self.divider = Divider::ItemCommaToolspecs;
        Ok(())
    }

    /// # Errors
    /// I/O, state machine errors
    fn want_message_item(&mut self, is_function: bool) -> Result<(), String> {
        match (&self.divider, is_function) {
            // Same section, just add comma
            (Divider::ItemCommaContent, false) | (Divider::ItemCommaFunctions, true) => {
                embedded_io::Write::write_all(&mut self.writer, b",\n")
                    .map_err(|e| format!("{e:?}"))?;
            }
            // The very beginning, write message content/function section
            // Assume that the attribute `role` has been written already, therefore add a comma
            (Divider::ItemNone, false) => {
                embedded_io::Write::write_all(&mut self.writer, b",\n\"content\":[\n")
                    .map_err(|e| format!("{e:?}"))?;
                self.divider = Divider::ItemCommaContent;
            }
            (Divider::ItemNone, true) => {
                embedded_io::Write::write_all(&mut self.writer, b",\n\"tool_calls\":[\n")
                    .map_err(|e| format!("{e:?}"))?;
                self.divider = Divider::ItemCommaFunctions;
            }
            // Switch between "content" to "tool_calls"
            (Divider::ItemCommaContent, true) | (Divider::ItemCommaFunctions, false) => {
                embedded_io::Write::write_all(&mut self.writer, b"]")
                    .map_err(|e| format!("{e:?}"))?;
                self.divider = Divider::ItemNone;
                self.want_message_item(is_function)?;
            }
            // Should start message first
            (Divider::ItemCommaToolspecs | Divider::MessageComma | Divider::Prologue, _) => {
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
    /// - I/O, state machine errors
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
        embedded_io::Write::write_all(&mut self.writer, br#"{"type":"#)
            .map_err(|e| format!("{e:?}"))?;
        let type_json = serde_json::to_string(write_item_type).map_err(|e| format!("{e:?}"))?;
        embedded_io::Write::write_all(&mut self.writer, type_json.as_bytes())
            .map_err(|e| format!("{e:?}"))?;

        for (key, value) in attrs {
            if key == "type" {
                continue;
            }
            if item_type == "image" && (key == "detail" || key == "content_type") {
                continue;
            }
            let key_part = format!(r#","{key}":"#);
            embedded_io::Write::write_all(&mut self.writer, key_part.as_bytes())
                .map_err(|e| format!("{e:?}"))?;
            let value_json = serde_json::to_string(value).map_err(|e| format!("{e:?}"))?;
            embedded_io::Write::write_all(&mut self.writer, value_json.as_bytes())
                .map_err(|e| format!("{e:?}"))?;
        }

        self.item_attr_mode = ItemAttrMode::Passthrough;
        Ok(())
    }

    fn end_item_logic(&mut self) -> Result<(), String> {
        let is_ctl = self
            .item_attr
            .as_ref()
            .and_then(|attrs| attrs.get("type"))
            .is_none_or(|t| t == "ctl");

        if !is_ctl {
            if self.item_attr_mode == ItemAttrMode::Collect {
                self.begin_item_logic()?;
            }
            embedded_io::Write::write_all(&mut self.writer, b"}").map_err(|e| format!("{e:?}"))?;
        }

        self.item_attr = None;
        self.item_attr_mode = ItemAttrMode::RaiseError;
        Ok(())
    }

    // ----------------------------------------------------
    // Action-triggering item content
    //

    /// # Errors
    /// - I/O, state machine errors
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

        self.begin_message(role)?;

        // If role is "tool", require and write tool_call_id at message level
        if role == "tool" {
            let tool_call_id = self
                .item_attr
                .as_mut()
                .and_then(|attrs| attrs.remove("tool_call_id"));
            match tool_call_id {
                Some(id) => {
                    embedded_io::Write::write_all(&mut self.writer, br#","tool_call_id":"#)
                        .map_err(|e| format!("{e:?}"))?;
                    let id_json = serde_json::to_string(&id).map_err(|e| format!("{e:?}"))?;
                    embedded_io::Write::write_all(&mut self.writer, id_json.as_bytes())
                        .map_err(|e| format!("{e:?}"))?;
                }
                None => {
                    return Err(
                        "Missing required 'tool_call_id' attribute for role 'tool'".to_string()
                    );
                }
            }
        }

        Ok(())
    }

    /// # Errors
    /// - I/O, state machine errors
    /// - for "type" attribute, the value is unknown or conflicting with the already typed item
    pub fn add_item_attribute(&mut self, key: String, value: String) -> Result<(), String> {
        if let ItemAttrMode::RaiseError = self.item_attr_mode {
            return Err("Content item is not started".to_string());
        }

        if self.item_attr_mode == ItemAttrMode::Passthrough {
            let key_part = format!(r#","{key}":"#);
            embedded_io::Write::write_all(&mut self.writer, key_part.as_bytes())
                .map_err(|e| format!("{e:?}"))?;
            let value_json = serde_json::to_string(&value).map_err(|e| format!("{e:?}"))?;
            embedded_io::Write::write_all(&mut self.writer, value_json.as_bytes())
                .map_err(|e| format!("{e:?}"))?;
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
    /// - I/O, state machine errors
    pub fn begin_text(&mut self) -> Result<(), String> {
        if let ItemAttrMode::RaiseError = self.item_attr_mode {
            return Err("Content item is not started".to_string());
        }
        self.add_item_attribute(String::from("type"), String::from("text"))?;
        self.begin_item_logic()?;

        embedded_io::Write::write_all(&mut self.writer, br#","text":""#)
            .map_err(|e| format!("{e:?}"))?;
        Ok(())
    }

    /// # Errors
    /// - I/O, state machine errors
    pub fn end_text(&mut self) -> Result<(), String> {
        embedded_io::Write::write_all(&mut self.writer, b"\"").map_err(|e| format!("{e:?}"))?;
        Ok(())
    }

    /// # Errors
    /// - I/O, state machine errors
    pub fn begin_image_url(&mut self) -> Result<(), String> {
        if let ItemAttrMode::RaiseError = self.item_attr_mode {
            return Err("Content item is not started".to_string());
        }
        self.add_item_attribute(String::from("type"), String::from("image"))?;
        self.begin_item_logic()?;

        embedded_io::Write::write_all(&mut self.writer, b",\"image_url\":{")
            .map_err(|e| format!("{e:?}"))?;
        if let Some(ref attrs) = self.item_attr {
            if let Some(ref detail) = attrs.get("detail") {
                embedded_io::Write::write_all(&mut self.writer, br#""detail":"#)
                    .map_err(|e| format!("{e:?}"))?;
                let detail_json = serde_json::to_string(detail).map_err(|e| format!("{e:?}"))?;
                embedded_io::Write::write_all(&mut self.writer, detail_json.as_bytes())
                    .map_err(|e| format!("{e:?}"))?;
                embedded_io::Write::write_all(&mut self.writer, b",")
                    .map_err(|e| format!("{e:?}"))?;
            }
        }
        embedded_io::Write::write_all(&mut self.writer, br#""url":""#)
            .map_err(|e| format!("{e:?}"))?;
        Ok(())
    }

    /// # Errors
    /// - I/O, state machine errors
    pub fn end_image_url(&mut self) -> Result<(), String> {
        embedded_io::Write::write_all(&mut self.writer, b"\"}").map_err(|e| format!("{e:?}"))?;
        Ok(())
    }

    /// # Errors
    /// - I/O, state machine errors
    pub fn image_key(&mut self, key: &str) -> Result<(), String> {
        let err_kind_to_str = |e: embedded_io::ErrorKind| format!("image key `{key}`: {e:?}");
        self.begin_image_url()?;
        embedded_io::Write::write_all(&mut self.writer, b"data:")
            .map_err(|e| format!("image key `{key}`: {e:?}"))?;
        if let Some(ref attrs) = self.item_attr {
            if let Some(content_type) = attrs.get("content_type") {
                // Serialize content_type as a JSON string, then strip outer quotes
                // This ensures proper escaping of special characters
                let json_str = serde_json::to_string(content_type)
                    .map_err(|e| format!("image key `{key}`: {e:?}"))?;
                // Remove the outer quotes that serde_json adds
                let inner = &json_str[1..json_str.len() - 1];
                embedded_io::Write::write_all(&mut self.writer, inner.as_bytes())
                    .map_err(|e| format!("image key `{key}`: {e:?}"))?;
            }
        }
        embedded_io::Write::write_all(&mut self.writer, b";base64,")
            .map_err(|e| format!("image key `{key}`: {e:?}"))?;

        let cname = std::ffi::CString::new(key).map_err(|e| format!("{e:?}"))?;
        let mut blob_reader = AReader::new(&cname).map_err(err_kind_to_str)?;

        // Read all data and encode as base64
        let mut data = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            match embedded_io::Read::read(&mut blob_reader, &mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    data.extend_from_slice(&buffer[..n]);
                }
                Err(e) => return Err(format!("image key `{key}`: {e:?}")),
            }
        }

        // Encode to base64 and write
        let encoded = STANDARD.encode(&data);
        embedded_io::Write::write_all(&mut self.writer, encoded.as_bytes())
            .map_err(|e| format!("image key `{key}`: {e:?}"))?;

        self.end_image_url()
    }

    /// # Errors
    /// - I/O, state machine errors
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

        embedded_io::Write::write_all(&mut self.writer, br#","function":{"name":"#)
            .map_err(|e| format!("{e:?}"))?;
        let name_json = serde_json::to_string(&name).map_err(|e| format!("{e:?}"))?;
        embedded_io::Write::write_all(&mut self.writer, name_json.as_bytes())
            .map_err(|e| format!("{e:?}"))?;
        embedded_io::Write::write_all(&mut self.writer, br#","arguments":""#)
            .map_err(|e| format!("{e:?}"))?;
        Ok(())
    }

    /// # Errors
    /// - I/O, state machine errors
    pub fn end_function_arguments(&mut self) -> Result<(), String> {
        embedded_io::Write::write_all(&mut self.writer, b"\"}").map_err(|e| format!("{e:?}"))?;
        Ok(())
    }

    /// # Errors
    /// - I/O
    /// - state machine errors
    /// - file not found
    /// - invalid JSON in file
    pub fn toolspec_key(&mut self, key: &str) -> Result<(), String> {
        let err_kind_to_str = |e: embedded_io::ErrorKind| format!("toolspec key `{key}`: {e:?}");
        let cname = std::ffi::CString::new(key).map_err(|e| format!("{e:?}"))?;
        let mut blob_reader = AReader::new(&cname).map_err(err_kind_to_str)?;
        let mut buffer = [0u8; 1024];
        let mut rjiter = scan_json::RJiter::new(&mut blob_reader, &mut buffer);

        self.toolspec_rjiter_with_key(&mut rjiter, Some(key))
    }

    /// # Errors
    /// - I/O
    /// - state machine errors
    /// - invalid JSON in rjiter
    pub fn toolspec_rjiter<R: embedded_io::Read>(
        &mut self,
        rjiter: &mut scan_json::RJiter<R>,
    ) -> Result<(), String> {
        self.toolspec_rjiter_with_key(rjiter, None)
    }

    /// # Errors
    /// - I/O
    /// - state machine errors
    /// - invalid JSON in rjiter
    fn toolspec_rjiter_with_key<R: embedded_io::Read>(
        &mut self,
        rjiter: &mut scan_json::RJiter<R>,
        key: Option<&str>,
    ) -> Result<(), String> {
        if let ItemAttrMode::RaiseError = self.item_attr_mode {
            return Err("Content item is not started".to_string());
        }
        self.add_item_attribute(String::from("type"), String::from("toolspec"))?;
        self.begin_item_logic()?;

        embedded_io::Write::write_all(&mut self.writer, br#","function":"#)
            .map_err(|e| format!("{e:?}"))?;

        let writer = self.get_writer();

        let error_prefix = if let Some(k) = key {
            format!("toolspec key `{k}`")
        } else {
            "toolspec rjiter".to_string()
        };

        // Create working buffer for context stack (512 bytes, up to 20 nesting levels)
        let mut working_buffer = [0u8; 512];
        let mut context = u8pool::U8Pool::new(&mut working_buffer, 20)
            .map_err(|e| format!("{error_prefix}: failed to create context pool: {e:?}"))?;

        scan_json::idtransform::idtransform(rjiter, writer, &mut context)
            .map_err(|e| format!("{error_prefix}: {e:?}"))?;

        Ok(())
    }
}
