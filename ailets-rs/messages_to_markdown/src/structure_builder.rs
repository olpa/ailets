use std::io::Write;
use awriter::AWriter;

pub struct StructureBuilder {
    awriter: AWriter,
    need_para_divider: bool,
}

impl StructureBuilder {
    pub fn new(awriter: AWriter) -> Self {
        StructureBuilder {
            awriter,
            need_para_divider: false,
        }
    }

    #[must_use]
    pub fn get_awriter(&mut self) -> &mut AWriter {
        &mut self.awriter
    }

    pub fn start_paragraph(&mut self) {
        if self.need_para_divider {
            self.str("\n\n");
        }
        self.need_para_divider = true;
    }

    pub fn str(&mut self, text: &str) {
        let text_bytes = text.as_bytes();
        self.awriter.write_all(text_bytes).unwrap();
    }

    pub fn finish_with_newline(&mut self) {
        if self.need_para_divider {
            self.str("\n");
        }
        self.need_para_divider = false;
    }
}
