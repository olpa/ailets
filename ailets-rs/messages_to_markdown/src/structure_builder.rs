use std::io::Write;

pub struct StructureBuilder<W: Write> {
    writer: W,
    need_para_divider: bool,
}

impl<W: Write> StructureBuilder<W> {
    pub fn new(writer: W) -> Self {
        StructureBuilder {
            writer,
            need_para_divider: false,
        }
    }

    #[must_use]
    pub fn get_writer(&mut self) -> &mut W {
        &mut self.writer
    }

    pub fn start_paragraph(&mut self) {
        if self.need_para_divider {
            self.writer.write_all(b"\n\n").unwrap();
        }
        self.need_para_divider = true;
    }

    pub fn finish_with_newline(&mut self) {
        if self.need_para_divider {
            self.writer.write_all(b"\n").unwrap();
        }
        self.need_para_divider = false;
    }
}
