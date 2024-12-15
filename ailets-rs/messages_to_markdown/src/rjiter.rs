use std::io::Read;

use jiter::{Jiter, JiterResult, JsonValue};

pub type Peek = jiter::Peek;

pub struct RJiter<'rj> {
    jiter: Jiter<'rj>,
    pos_before_call_jiter: usize,
    reader: &'rj mut dyn Read,
    buffer: &'rj [u8],
    bytes_in_buffer: usize,
}

impl<'rj> RJiter<'rj> {
    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::missing_panics_doc)]
    pub fn new(reader: &'rj mut dyn Read, buffer: &'rj mut [u8]) -> Self {
        let bytes_in_buffer = reader.read(buffer).unwrap();
        let jiter_buffer = &buffer[..bytes_in_buffer];

        RJiter {
            jiter: Jiter::new(jiter_buffer),
            pos_before_call_jiter: 0,
            reader,
            buffer,
            bytes_in_buffer,
        }
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn peek(&mut self) -> JiterResult<Peek> {
        self.jiter.peek()
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn next_array(&mut self) -> JiterResult<Option<Peek>> {
        self.jiter.next_array()
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn array_step(&mut self) -> JiterResult<Option<Peek>> {
        self.jiter.array_step()
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn next_object_bytes(&mut self) -> JiterResult<Option<&[u8]>> {
        self.jiter.next_object_bytes()
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn next_skip(&mut self) -> JiterResult<()> {
        self.jiter.next_skip()
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn next_str(&mut self) -> JiterResult<&str> {
        self.jiter.next_str()
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn finish(&mut self) -> JiterResult<()> {
        self.jiter.finish()
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn next_key_bytes(&mut self) -> JiterResult<Option<&[u8]>> {
        self.jiter.next_key_bytes()
    }

    #[allow(clippy::missing_errors_doc)]
    pub fn next_value(&mut self) -> JiterResult<JsonValue<'rj>> {
        loop {
            self.on_before_call_jiter();
            let result = self.jiter.next_value();
            if result.is_ok() {
                return result;
            }
            if !self.feed() {
                return result;
            }
        }
    }

    fn on_before_call_jiter(&mut self) {
        self.pos_before_call_jiter = self.jiter.current_index();
    }

    pub fn feed(&mut self) -> bool {
        let pos = self.pos_before_call_jiter;

        //
        // Skip whitespaces
        //
        let skip_ws_parser = Jiter::Parser::new(self.buffer[pos..]);
        skip_ws_parser.finish();
        let pos = pos + skip_ws_parser.current_index();

        //
        // Copy remaining bytes to the beginning of the buffer
        //
        if pos > 0 {
            self.buffer.copy_within(pos..self.bytes_in_buffer, 0);
            self.bytes_in_buffer -= pos;
        }

        //
        // Read new bytes
        //
        let n_new_bytes = self.reader.read(self.buffer[self.bytes_in_buffer..]).unwrap();
        self.bytes_in_buffer += n_new_bytes;

        //
        // Create new Jiter and inform caller if any new bytes were read
        //
        self.jiter = Jiter::new(self.buffer[..self.bytes_in_buffer]);
        return n_new_bytes > 0;
    }
}

