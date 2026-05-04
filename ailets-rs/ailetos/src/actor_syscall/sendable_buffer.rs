#[cfg(debug_assertions)]
use tracing::error;

/// A raw slice pointer that can be sent between threads.
///
/// SAFETY: Safe only when the sender blocks until the receiver sends a response, ensuring:
/// 1. The buffer remains valid (stack frame doesn't unwind)
/// 2. No concurrent access (sender is blocked)
/// 3. Proper synchronization (channel enforces happens-before)
/// 4. The pointer is consumed exactly once via `into_raw()`
pub struct SendablePtr<P: Copy> {
    ptr: P,
    #[cfg(debug_assertions)]
    consumed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl<P: Copy> SendablePtr<P> {
    /// # Safety
    ///
    /// Caller must ensure:
    /// 1. The pointer remains valid until consumed via `into_raw()`
    /// 2. The caller will block waiting for a response before the buffer goes out of scope
    /// 3. No other references to this buffer exist during the async operation
    /// 4. The pointer is consumed exactly once via `into_raw()`
    unsafe fn new_inner(ptr: P) -> Self {
        Self {
            ptr,
            #[cfg(debug_assertions)]
            consumed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Consume the pointer and return the raw value.
    ///
    /// If the pointer has already been consumed, logs a critical error but still returns
    /// the pointer. This violates the safety contract and indicates a serious programming error.
    #[must_use]
    pub fn into_raw(self) -> P {
        #[cfg(debug_assertions)]
        {
            let already_consumed = self
                .consumed
                .swap(true, std::sync::atomic::Ordering::SeqCst);
            if already_consumed {
                error!(
                    "CRITICAL: SendablePtr used twice - safety contract violated! \
                     This may lead to use-after-free bugs."
                );
            }
        }
        self.ptr
    }
}

pub type SendableMutPtr = SendablePtr<*mut [u8]>;
pub type SendableConstPtr = SendablePtr<*const [u8]>;

impl SendableMutPtr {
    /// # Safety
    ///
    /// See `SendablePtr::new_inner`.
    pub unsafe fn new(buffer: &mut [u8]) -> Self {
        unsafe { Self::new_inner(std::ptr::from_mut::<[u8]>(buffer)) }
    }
}

impl SendableConstPtr {
    /// # Safety
    ///
    /// See `SendablePtr::new_inner`.
    #[must_use]
    pub unsafe fn new(buffer: &[u8]) -> Self {
        unsafe { Self::new_inner(std::ptr::from_ref::<[u8]>(buffer)) }
    }
}

// SAFETY: See SendablePtr documentation above
unsafe impl Send for SendableMutPtr {}
unsafe impl Send for SendableConstPtr {}
