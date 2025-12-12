use std::os::raw::c_int;

/// Trait for actor runtime operations.
/// Provides an abstraction layer over the underlying I/O and actor runtime functions.
/// This allows for both FFI-based implementations (WASM) and native Rust implementations (testing, CLI).
pub trait ActorRuntime {
    /// Get the last error number
    fn get_errno(&self) -> c_int;

    /// Open a stream for reading
    fn open_read(&self, name: &str) -> c_int;

    /// Open a stream for writing
    fn open_write(&self, name: &str) -> c_int;

    /// Read from a file descriptor into the provided buffer
    fn aread(&self, fd: c_int, buffer: &mut [u8]) -> c_int;

    /// Write from the provided buffer to a file descriptor
    fn awrite(&self, fd: c_int, buffer: &[u8]) -> c_int;

    /// Close a file descriptor
    fn aclose(&self, fd: c_int) -> c_int;
}
