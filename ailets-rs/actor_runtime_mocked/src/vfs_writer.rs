//! A writer implementation that writes to the VFS for value nodes.

use crate::Vfs;
use std::cell::RefCell;
use std::io::{Result, Write};
use std::rc::Rc;

pub struct VfsWriter {
    vfs: Rc<RefCell<Vfs>>,
    filename: String,
}

impl VfsWriter {
    pub fn new(vfs: Rc<RefCell<Vfs>>, filename: String) -> Self {
        Self { vfs, filename }
    }

    /// Close the writer (AWriter-compatible interface)
    /// # Errors
    /// None
    pub fn close(self) -> Result<()> {
        // VFS doesn't need explicit closing
        Ok(())
    }
}

impl Write for VfsWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.vfs
            .borrow_mut()
            .append_to_file(&self.filename, buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        // VFS doesn't need explicit flushing
        Ok(())
    }
}

// Need to import the trait to implement it, but it's in gpt crate which depends on this crate
// We'll implement it in the test file instead where we have access to both crates
