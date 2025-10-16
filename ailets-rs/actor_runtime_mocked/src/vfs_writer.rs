//! A writer implementation that writes to the VFS for value nodes.

use crate::Vfs;
use std::cell::RefCell;
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
    pub fn close(self) -> Result<(), embedded_io::ErrorKind> {
        // VFS doesn't need explicit closing
        Ok(())
    }
}

impl embedded_io::ErrorType for VfsWriter {
    type Error = embedded_io::ErrorKind;
}

impl embedded_io::Write for VfsWriter {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.vfs
            .borrow_mut()
            .append_to_file(&self.filename, buf)
            .map_err(|_| embedded_io::ErrorKind::Other)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        // VFS doesn't need explicit flushing
        Ok(())
    }
}
