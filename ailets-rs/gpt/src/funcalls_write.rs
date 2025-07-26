//! Function call writing abstractions for streaming output
//!
//! This module provides traits and implementations for writing function call data
//! in a streaming fashion, enabling efficient processing of large function calls
//! while maintaining JSON safety through proper escaping.

use std::io::Write;

// =============================================================================
// Types and Utilities
// =============================================================================

/// Result type for function call writing operations
type FunCallResult = Result<(), Box<dyn std::error::Error>>;

/// Escapes a string for safe inclusion in JSON by escaping backslashes and quotes
/// 
/// # Arguments
/// * `s` - The string to escape
/// 
/// # Returns
/// A new string with `\` escaped as `\\` and `"` escaped as `\"`
fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// =============================================================================
// Core Trait Definition
// =============================================================================

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
/// # Example
/// ```rust,ignore
/// let mut writer = FunCallsToChat::new(output);
/// writer.new_item("call_123", "my_function")?;
/// writer.arguments_chunk("{\"param1\":")?;
/// writer.arguments_chunk("\"value1\"}")?;
/// writer.end_item()?;
/// writer.end()?;
/// ```
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
    fn arguments_chunk(&mut self, chunk: &str) -> FunCallResult;

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

// =============================================================================
// Chat Writer Implementation
// =============================================================================

/// Chat-style function call writer for JSON output
///
/// Writes function calls in JSON format suitable for chat systems.
/// Each function call is written as a single JSON line in the format:
/// `[{"type":"function","id":"...","name":"..."},{"arguments":"..."}]`
///
/// All string values (id, name, arguments) are properly JSON-escaped to prevent
/// injection attacks and ensure valid JSON output.
///
/// # Type Parameters
/// * `W` - Any type implementing `std::io::Write`
pub struct FunCallsToChat<W: Write> {
    writer: W,
}

impl<W: Write> FunCallsToChat<W> {
    /// Creates a new chat-style function call writer
    ///
    /// # Arguments
    /// * `writer` - The underlying writer for output
    /// 
    /// # Returns
    /// A new `FunCallsToChat` instance that will write JSON-formatted function calls
    #[must_use]
    pub const fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: Write> FunCallsWrite for FunCallsToChat<W> {
    fn new_item(&mut self, id: &str, name: &str) -> FunCallResult {
        // Write the JSON structure opening with escaped id and name
        write!(
            self.writer,
            r#"[{{"type":"function","id":"{}","name":"{}"}},{{"arguments":""#,
            escape_json_string(id),
            escape_json_string(name)
        )?;
        Ok(())
    }

    fn arguments_chunk(&mut self, chunk: &str) -> FunCallResult {
        // Write the escaped argument chunk directly into the JSON string
        write!(self.writer, "{}", escape_json_string(chunk))?;
        Ok(())
    }

    fn end_item(&mut self) -> FunCallResult {
        // Close the arguments string and the JSON array with newline
        writeln!(self.writer, "\"}}]")?;
        Ok(())
    }

    fn end(&mut self) -> FunCallResult {
        // No additional cleanup required for chat format
        Ok(())
    }
}

// =============================================================================
// Composite Writer Implementation
// =============================================================================

/// Composite writer that forwards function calls to multiple destinations
///
/// This writer combines chat-style JSON output with DAG operations, enabling
/// function calls to be processed simultaneously for:
/// - Chat display (JSON format)
/// - Internal DAG tracking and workflow management
///
/// The composite pattern ensures both writers receive the same data in the
/// correct order, with fail-fast error handling.
///
/// # Type Parameters
/// * `W` - Writer type for chat output (must implement `std::io::Write`)
/// * `T` - DAG operations trait implementation
pub struct FunCallsGpt<'a, W: Write, T: crate::dagops::DagOpsTrait> {
    chat_writer: FunCallsToChat<W>,
    dag_writer: crate::dagops::DagOpsWrite<'a, T>,
}

impl<'a, W: Write, T: crate::dagops::DagOpsTrait> FunCallsGpt<'a, W, T> {
    /// Create a new composite function call writer
    ///
    /// # Arguments
    /// * `writer` - Output writer for chat-style JSON formatting
    /// * `dagops` - Mutable reference to DAG operations handler
    /// 
    /// # Returns
    /// A composite writer that will forward all operations to both the chat
    /// writer and the DAG writer simultaneously
    pub fn new(writer: W, dagops: &'a mut T) -> Self {
        Self {
            chat_writer: FunCallsToChat::new(writer),
            dag_writer: crate::dagops::DagOpsWrite::new(dagops),
        }
    }
}

impl<'a, W: Write, T: crate::dagops::DagOpsTrait> FunCallsWrite for FunCallsGpt<'a, W, T> {
    fn new_item(&mut self, id: &str, name: &str) -> FunCallResult {
        // Forward to both writers with fail-fast error handling
        self.chat_writer.new_item(id, name)?;
        self.dag_writer.new_item(id, name)?;
        Ok(())
    }

    fn arguments_chunk(&mut self, chunk: &str) -> FunCallResult {
        // Forward argument chunks to both writers
        self.chat_writer.arguments_chunk(chunk)?;
        self.dag_writer.arguments_chunk(chunk)?;
        Ok(())
    }

    fn end_item(&mut self) -> FunCallResult {
        // Finalize current item in both writers
        self.chat_writer.end_item()?;
        self.dag_writer.end_item()?;
        Ok(())
    }

    fn end(&mut self) -> FunCallResult {
        // Complete processing in both writers
        self.chat_writer.end()?;
        self.dag_writer.end()?;
        Ok(())
    }
}
