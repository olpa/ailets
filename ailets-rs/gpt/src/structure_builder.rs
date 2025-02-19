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

    pub fn end_message(&mut self) {
        if self.message_is_closed {
            return;
        }
        if !self.message_has_role && !self.message_has_content {
            return;
        }
        if !self.message_has_content {
            self.begin_content();
        }
        if self.text_is_open {
            self.str("\"}");
            self.text_is_open = false;
        }
        if self.message_has_content {
            self.str("]");
        }
        self.str("}\n");
        self.message_is_closed = true;
    }

    pub fn role(&mut self, role: &str) {
        if self.message_has_role {
            return;
        }
        self.str("{\"role\":\"");
        self.str(role);
        self.str("\"");
        self.message_has_role = true;
    }

    pub fn begin_content(&mut self) {
        if self.message_has_content {
            return;
        }
        if !self.message_has_role {
            self.role("assistant");
        }
        self.str(",\"content\":[");
        self.message_has_content = true;
        self.text_is_open = false;
    }

    pub fn begin_text_chunk(&mut self) {
        if !self.message_has_content {
            self.begin_content();
        }
        if !self.text_is_open {
            self.str("{\"type\":\"text\",\"text\":\"");
            self.text_is_open = true;
        }
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn str(&mut self, text: &str) {
        self.writer.write_all(text.as_bytes()).unwrap();
    }
}
