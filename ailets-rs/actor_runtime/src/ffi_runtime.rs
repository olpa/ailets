use crate::actor_runtime::{aclose, aread, awrite, get_errno, open_read, open_write};
use crate::runtime_trait::ActorRuntime;
use std::ffi::CString;
use std::os::raw::c_int;

/// FFI-based implementation of `ActorRuntime`.
/// Uses the C FFI functions for WASM targets.
pub struct FfiActorRuntime;

impl FfiActorRuntime {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for FfiActorRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ActorRuntime for FfiActorRuntime {
    fn get_errno(&self) -> c_int {
        unsafe { get_errno() }
    }

    fn open_read(&self, name: &str) -> c_int {
        let Ok(c_name) = CString::new(name) else {
            return -1; // Invalid name (contains null byte)
        };
        unsafe { open_read(c_name.as_ptr()) }
    }

    fn open_write(&self, name: &str) -> c_int {
        let Ok(c_name) = CString::new(name) else {
            return -1; // Invalid name (contains null byte)
        };
        unsafe { open_write(c_name.as_ptr()) }
    }

    fn aread(&self, fd: c_int, buffer: &mut [u8]) -> c_int {
        let count = u32::try_from(buffer.len()).unwrap_or(u32::MAX - 1);
        unsafe { aread(fd, buffer.as_mut_ptr(), count) }
    }

    fn awrite(&self, fd: c_int, buffer: &[u8]) -> c_int {
        let count = u32::try_from(buffer.len()).unwrap_or(u32::MAX - 1);
        unsafe { awrite(fd, buffer.as_ptr(), count) }
    }

    fn aclose(&self, fd: c_int) -> c_int {
        unsafe { aclose(fd) }
    }
}
