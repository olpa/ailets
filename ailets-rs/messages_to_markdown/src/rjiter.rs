use std::io::Read;
use std::io::Write;

use jiter::{Jiter, JiterResult, JsonValue};

pub type Peek = jiter::Peek;

pub struct RJiter<'rj> {
    jiter: Jiter<'rj>,
    pos_before_call_jiter: usize,
    reader: &'rj mut dyn Read,
    buffer: &'rj mut [u8],
    bytes_in_buffer: usize,
}

impl<'rj> RJiter<'rj> {
    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::missing_panics_doc)]
    pub fn new(reader: &'rj mut dyn Read, buffer: &'rj mut [u8]) -> Self {
        let bytes_in_buffer = reader.read(buffer).unwrap();
        let jiter_buffer = &buffer[..bytes_in_buffer];
        let rjiter_buffer = unsafe {
            #[allow(mutable_transmutes)]
            #[allow(clippy::transmute_ptr_to_ptr)]
            std::mem::transmute::<&[u8], &'rj mut [u8]>(buffer)
        };

        RJiter {
            jiter: Jiter::new(jiter_buffer).with_allow_partial_strings(),
            pos_before_call_jiter: 0,
            reader,
            buffer: rjiter_buffer,
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
    pub fn next_object(&mut self) -> JiterResult<Option<&str>> {
        self.jiter.next_object()
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

    #[allow(clippy::missing_errors_doc)]
    #[allow(clippy::missing_panics_doc)]
    pub fn write_bytes(&mut self, writer: &mut dyn Write) -> JiterResult<()> {
        loop {
            self.on_before_call_jiter();
            let result = self.jiter.known_bytes();
            if let Ok(bytes) = result {
                writer.write_all(bytes).unwrap();
                if self.jiter.current_index() <= self.bytes_in_buffer {
                    return Ok(());
                }
                self.on_before_call_jiter();
                if !self.feed_inner(true) {
                    return Ok(());
                }
            } else {
                return Err(result.unwrap_err());
            }
        }
    }

    fn on_before_call_jiter(&mut self) {
        self.pos_before_call_jiter = self.jiter.current_index();
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn feed(&mut self) -> bool {
        self.on_before_call_jiter();
        self.feed_inner(false)
    }

    fn feed_inner(&mut self, is_partial_string: bool) -> bool {
        let mut pos = self.pos_before_call_jiter;

        //
        // Skip whitespaces
        //
        if !is_partial_string && pos < self.bytes_in_buffer {
            let mut skip_ws_parser = Jiter::new(&self.buffer[pos..self.bytes_in_buffer]);
            let _ = skip_ws_parser.finish();
            pos += skip_ws_parser.current_index();
        }

        //
        // Copy remaining bytes to the beginning of the buffer
        //
        if pos > 0 {
            if pos < self.bytes_in_buffer {
                assert!(
                    !is_partial_string,
                    "Buffer should be completely consumed in partial string case"
                );
                self.buffer.copy_within(pos..self.bytes_in_buffer, 0);
                self.bytes_in_buffer -= pos;
            } else {
                self.bytes_in_buffer = 0;
            }
        } else {
            self.bytes_in_buffer = 0;
        }

        //
        // Read new bytes
        //
        let start_index = if is_partial_string {
            1
        } else {
            self.bytes_in_buffer
        };
        let n_new_bytes = self.reader.read(&mut self.buffer[start_index..]).unwrap();
        self.bytes_in_buffer += n_new_bytes;

        if is_partial_string {
            self.buffer[0] = 34;
            self.bytes_in_buffer += 1;
        }

        //
        // Create new Jiter and inform caller if any new bytes were read
        //
        let jiter_buffer_2 = &self.buffer[..self.bytes_in_buffer];
        let jiter_buffer = unsafe { std::mem::transmute::<&[u8], &'rj [u8]>(jiter_buffer_2) };
        self.jiter = Jiter::new(jiter_buffer).with_allow_partial_strings();

        n_new_bytes > 0
    }
}
