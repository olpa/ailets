//! A writer implementation that stores written data in memory for later inspection.
//!
//! # Example
//! ```
//! use actor_runtime_mocked::RcWriter;
//! use std::io::Write;
//!
//! let mut writer = RcWriter::new();
//! writer.write_all(b"Hello, world!").unwrap();
//! assert_eq!(writer.get_output(), "Hello, world!");
//! ```

use std::cell::RefCell;
use std::io::{Result, Write};
use std::rc::Rc;

#[derive(Clone, Default)]
pub struct RcWriter {
    inner: Rc<RefCell<Vec<u8>>>,
}

impl Write for RcWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.inner.borrow_mut().write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        self.inner.borrow_mut().flush()
    }
}

impl RcWriter {
    #[must_use]
    pub fn new() -> Self {
        RcWriter {
            inner: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[must_use]
    pub fn get_output(&self) -> String {
        String::from_utf8_lossy(&self.inner.borrow()).to_string()
    }
}
