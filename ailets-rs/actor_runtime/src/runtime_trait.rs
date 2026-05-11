/// Trait for actor runtime operations.
/// Provides an abstraction layer over the underlying I/O and actor runtime functions.
/// This allows for both FFI-based implementations (WASM) and native Rust implementations (testing, CLI).
///
/// All I/O operations return `Result<usize, i32>` where the error is a POSIX errno.
pub trait ActorRuntime {
    /// Open a stream for reading.
    /// Returns the file descriptor on success, or errno on failure.
    fn open_read(&self, name: &str) -> Result<isize, i32>;

    /// Open a stream for writing.
    /// Returns the file descriptor on success, or errno on failure.
    fn open_write(&self, name: &str) -> Result<isize, i32>;

    /// Read from a file descriptor into the provided buffer.
    /// Returns bytes read on success, or errno on failure.
    fn aread(&self, fd: isize, buffer: &mut [u8]) -> Result<usize, i32>;

    /// Write from the provided buffer to a file descriptor.
    /// Returns bytes written on success, or errno on failure.
    fn awrite(&self, fd: isize, buffer: &[u8]) -> Result<usize, i32>;

    /// Close a file descriptor.
    /// Returns `Ok(())` on success, or errno on failure.
    fn aclose(&self, fd: isize) -> Result<(), i32>;

    /// Get this actor's node handle (identity)
    fn node_handle(&self) -> i64;

    /// Self-suspend: block until the host calls resume for this actor.
    /// In test/mock environments, this may be a no-op.
    fn suspend_and_wait(&self);
}
