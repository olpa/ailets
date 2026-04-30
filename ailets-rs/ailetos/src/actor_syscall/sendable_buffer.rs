#[cfg(debug_assertions)]
use tracing::error;

/// A wrapper around a raw mutable slice pointer that can be sent between threads.
/// SAFETY: This is only safe because the sender (aread) blocks until the receiver
/// (`IoBridge` handler) sends a response, ensuring:
/// 1. The buffer remains valid (stack frame doesn't unwind)
/// 2. No concurrent access (sender is blocked)
/// 3. Proper synchronization (channel enforces happens-before)
pub struct SendableBuffer {
    ptr: *mut [u8],
    #[cfg(debug_assertions)]
    consumed: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl SendableBuffer {
    /// Create a new `SendableBuffer` from a mutable slice reference.
    ///
    /// # Safety
    ///
    /// Caller must ensure:
    /// 1. The pointer remains valid until consumed via `into_raw()`
    /// 2. The caller will block waiting for a response before the buffer goes out of scope
    /// 3. No other references to this buffer exist during the async operation
    /// 4. The `SendableBuffer` is consumed exactly once via `into_raw()`
    pub unsafe fn new(buffer: &mut [u8]) -> Self {
        Self {
            ptr: std::ptr::from_mut::<[u8]>(buffer),
            #[cfg(debug_assertions)]
            consumed: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Consume the `SendableBuffer` and return the raw pointer.
    /// This prevents accidental reuse of the same buffer.
    ///
    /// If the buffer has already been consumed, logs a critical error but still returns
    /// the pointer. This violates the safety contract and indicates a serious programming error.
    #[must_use]
    pub fn into_raw(self) -> *mut [u8] {
        #[cfg(debug_assertions)]
        {
            let already_consumed = self
                .consumed
                .swap(true, std::sync::atomic::Ordering::SeqCst);
            if already_consumed {
                error!(
                    "CRITICAL: SendableBuffer used twice - safety contract violated! \
                     This may lead to use-after-free or double-free bugs."
                );
            }
        }
        self.ptr
    }
}

// SAFETY: See SendableBuffer documentation above
unsafe impl Send for SendableBuffer {}
