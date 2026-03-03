/// Trait for actor runtime operations.
/// Provides an abstraction layer over the underlying I/O and actor runtime functions.
/// This allows for both FFI-based implementations (WASM) and native Rust implementations (testing, CLI).
///
/// Uses `isize` for all I/O operations and file descriptors to match Rust's native signed integer type
/// and avoid unnecessary conversions. On wasm32 targets, `isize` is equivalent to `i32`.
pub trait ActorRuntime {
    /// Get the last error number
    fn get_errno(&self) -> isize;

    /// Open a stream for reading
    fn open_read(&self, name: &str) -> isize;

    /// Open a stream for writing
    fn open_write(&self, name: &str) -> isize;

    /// Read from a file descriptor into the provided buffer
    fn aread(&self, fd: isize, buffer: &mut [u8]) -> isize;

    /// Write from the provided buffer to a file descriptor
    fn awrite(&self, fd: isize, buffer: &[u8]) -> isize;

    /// Close a file descriptor
    fn aclose(&self, fd: isize) -> isize;
}
