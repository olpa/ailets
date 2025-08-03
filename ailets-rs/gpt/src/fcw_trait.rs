//! Function call writing trait definition
//!
//! This module provides the core trait for writing function call data
//! in a streaming fashion, enabling efficient processing of large function calls
//! while maintaining JSON safety through proper escaping.

/// Result type for function call writing operations
pub type FunCallResult = Result<(), Box<dyn std::error::Error>>;

/// Trait for writing function call data in a streaming manner
///
/// This trait supports streaming output by breaking function calls into discrete phases:
/// 1. `new_item` - Initialize a new function call with ID and name
/// 2. `arguments_chunk` - Stream argument data in chunks (can be called multiple times)
/// 3. `end_item` - Finalize the current function call
/// 4. `end` - Complete all processing
///
/// The streaming approach allows for efficient processing of large function calls
/// without requiring the entire payload to be loaded into memory at once.
///
pub trait FunCallsWrite {
    /// Initialize a new function call with the given ID and name
    ///
    /// # Arguments
    /// * `id` - Unique identifier for the function call (will be JSON-escaped)
    /// * `name` - Name of the function to be called (will be JSON-escaped)
    ///
    /// # Errors
    /// Returns an error if the underlying writer fails
    fn new_item(&mut self, id: &str, name: &str) -> FunCallResult;

    /// Add a chunk of arguments to the current function call
    ///
    /// This method can be called multiple times to stream large arguments.
    /// All chunks will be concatenated and JSON-escaped in the final output.
    ///
    /// # Arguments
    /// * `chunk` - The arguments chunk to append (will be JSON-escaped)
    ///
    /// # Errors
    /// Returns an error if the underlying writer fails
    fn arguments_chunk(&mut self, chunk: &[u8]) -> FunCallResult;

    /// Finalize the current function call item
    ///
    /// This must be called after all argument chunks have been written.
    ///
    /// # Errors
    /// Returns an error if the underlying writer fails
    fn end_item(&mut self) -> FunCallResult;

    /// Complete all function call processing
    ///
    /// This is called once at the end of all function call processing.
    ///
    /// # Errors
    /// Returns an error if the underlying writer fails
    fn end(&mut self) -> FunCallResult;
}
