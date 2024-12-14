use std::io::Read;

use jiter::{Jiter, JiterResult};

pub type Peek = jiter::Peek;

pub struct RJiter<'rj> {
    jiter: Jiter<'rj>,
    // reader: &'rj mut dyn Read,
    // buffer: &'rj [u8],
}

impl<'rj> RJiter<'rj> {
    pub fn new(reader: &'rj mut dyn Read, buffer: &'rj mut [u8]) -> Self {
        let mut pos = 0;
        loop {
            let buf_mut = &mut buffer[pos..];
            let bytes_read = reader.read(buf_mut).unwrap();
            if bytes_read == 0 {
                break;
            }
            pos += bytes_read;
        }
        let buffer = &buffer[..pos];

        // RJiter { jiter: Jiter::new(buffer), reader, buffer }
        RJiter {
            jiter: Jiter::new(buffer),
        }
    }

    pub fn peek(&mut self) -> JiterResult<Peek> {
        self.jiter.peek()
    }

    pub fn next_array(&mut self) -> JiterResult<Option<Peek>> {
        self.jiter.next_array()
    }

    pub fn array_step(&mut self) -> JiterResult<Option<Peek>> {
        self.jiter.array_step()
    }

    pub fn next_object_bytes(&mut self) -> JiterResult<Option<&[u8]>> {
        self.jiter.next_object_bytes()
    }

    pub fn next_skip(&mut self) -> JiterResult<()> {
        self.jiter.next_skip()
    }

    pub fn next_str(&mut self) -> JiterResult<&str> {
        self.jiter.next_str()
    }

    pub fn finish(&mut self) -> JiterResult<()> {
        self.jiter.finish()
    }

    pub fn next_key_bytes(&mut self) -> JiterResult<Option<&[u8]>> {
        self.jiter.next_key_bytes()
    }
}
