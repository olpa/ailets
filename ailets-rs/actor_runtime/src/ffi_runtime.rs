use crate::actor_runtime::{
    aclose, aread, awrite, get_errno, get_node_handle, open_read, open_write, suspend_and_wait,
};
use crate::runtime_trait::ActorRuntime;
use std::ffi::CString;

/// FFI-based implementation of `ActorRuntime`.
/// Uses the C FFI functions for WASM targets.
///
/// Receives a `BlockingActorRuntime` threaded through FFI glue and exposes
/// it to WebAssembly actors as `FfiActorRuntime`.
pub struct FfiActorRuntime;

impl FfiActorRuntime {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Convert FFI result to `Result`, fetching errno on failure.
    #[allow(clippy::cast_possible_truncation)] // errno values are small
    fn to_result(result: isize) -> Result<usize, i32> {
        if result < 0 {
            Err(unsafe { get_errno() } as i32)
        } else {
            #[allow(clippy::cast_sign_loss)]
            Ok(result as usize)
        }
    }
}

impl Default for FfiActorRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ActorRuntime for FfiActorRuntime {
    #[allow(clippy::cast_possible_truncation)] // errno values are small
    fn open_read(&self, name: &str) -> Result<isize, i32> {
        const EINVAL: i32 = 22;
        let Ok(c_name) = CString::new(name) else {
            return Err(EINVAL);
        };
        let fd = unsafe { open_read(c_name.as_ptr()) };
        if fd < 0 {
            Err(unsafe { get_errno() } as i32)
        } else {
            Ok(fd)
        }
    }

    #[allow(clippy::cast_possible_truncation)] // errno values are small
    fn open_write(&self, name: &str) -> Result<isize, i32> {
        const EINVAL: i32 = 22;
        let Ok(c_name) = CString::new(name) else {
            return Err(EINVAL);
        };
        let fd = unsafe { open_write(c_name.as_ptr()) };
        if fd < 0 {
            Err(unsafe { get_errno() } as i32)
        } else {
            Ok(fd)
        }
    }

    fn aread(&self, fd: isize, buffer: &mut [u8]) -> Result<usize, i32> {
        let count = u32::try_from(buffer.len()).unwrap_or(u32::MAX - 1);
        let result = unsafe { aread(fd, buffer.as_mut_ptr(), count) };
        Self::to_result(result)
    }

    fn awrite(&self, fd: isize, buffer: &[u8]) -> Result<usize, i32> {
        let count = u32::try_from(buffer.len()).unwrap_or(u32::MAX - 1);
        let result = unsafe { awrite(fd, buffer.as_ptr(), count) };
        Self::to_result(result)
    }

    #[allow(clippy::cast_possible_truncation)] // errno values are small
    fn aclose(&self, fd: isize) -> Result<(), i32> {
        let result = unsafe { aclose(fd) };
        if result < 0 {
            Err(unsafe { get_errno() } as i32)
        } else {
            Ok(())
        }
    }

    fn node_handle(&self) -> i64 {
        unsafe { get_node_handle() }
    }

    fn listdir(&self, _dir: &str) -> Result<Vec<String>, i32> {
        Err(38) // ENOSYS
    }

    fn suspend_and_wait(&self) {
        unsafe { suspend_and_wait() }
    }
}
