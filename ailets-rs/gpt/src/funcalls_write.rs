//! Function call writing abstractions for streaming output
//!
//! This module provides traits and implementations for writing function call data
//! in a streaming fashion, allowing for efficient processing of large function calls.

use std::io::Write;

/// Result type for function call writing operations
type FunCallResult = Result<(), Box<dyn std::error::Error>>;

/// Escapes a string for safe inclusion in JSON
fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Trait for writing function call data in a streaming manner
///
/// This trait supports streaming output by breaking function calls into discrete phases:
/// 1. `new_item` - Initialize a new function call
/// 2. `arguments_chunk` - Stream argument data in chunks (can be called multiple times)
/// 3. `end_item` - Finalize the function call
/// 4. `end` - Complete all processing
///
/// # Example
/// ```rust,ignore
/// let mut writer = FunCallsToChat::new(output);
/// writer.new_item("call_123", "my_function")?;
/// writer.arguments_chunk("{\"param1\":")?;
/// writer.arguments_chunk("\"value1\"}")?;
/// writer.end_item()?;
/// ```
pub trait FunCallsWrite {
    /// Initialize a new function call
    ///
    /// # Arguments
    /// * `id` - Unique identifier for the function call
    /// * `name` - Name of the function to be called
    fn new_item(&mut self, id: &str, name: &str) -> FunCallResult;

    /// Add a chunk of arguments to the current function call
    ///
    /// This method can be called multiple times to stream large arguments.
    /// All chunks will be concatenated in the final output.
    ///
    /// # Arguments
    /// * `chunk` - The arguments chunk to append
    fn arguments_chunk(&mut self, chunk: &str) -> FunCallResult;

    /// Finalize the current function call item
    ///
    /// This must be called after all argument chunks have been written.
    fn end_item(&mut self) -> FunCallResult;

    /// Complete all function call processing
    ///
    /// This is called once at the end of all function call processing.
    fn end(&mut self) -> FunCallResult;
}

/// Chat-style function call writer
///
/// Writes function calls in JSON format suitable for chat systems.
/// Each function call is written as a single JSON line in the format:
/// `[{"type":"function","id":"...","name":"..."},{"arguments":"..."}]`
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
    #[must_use]
    pub const fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: Write> FunCallsWrite for FunCallsToChat<W> {
    fn new_item(&mut self, id: &str, name: &str) -> FunCallResult {
        // Start writing the function call JSON structure with properly escaped values
        write!(
            self.writer,
            r#"[{{"type":"function","id":"{}","name":"{}"}},{{"arguments":""#,
            escape_json_string(id),
            escape_json_string(name)
        )?;
        Ok(())
    }

    fn arguments_chunk(&mut self, chunk: &str) -> FunCallResult {
        // Escape the chunk as a JSON string value when embedding in JSON
        write!(self.writer, "{}", escape_json_string(chunk))?;
        Ok(())
    }

    fn end_item(&mut self) -> FunCallResult {
        writeln!(self.writer, "\"}}]")?;
        Ok(())
    }

    fn end(&mut self) -> FunCallResult {
        // No special cleanup needed for chat format
        Ok(())
    }
}

/// Composite writer that forwards function calls to multiple destinations
///
/// This writer combines chat-style output with DAG operations, allowing
/// function calls to be processed for both chat display and internal DAG tracking.
///
/// # Type Parameters
/// * `W` - Writer type for chat output
/// * `T` - DAG operations trait implementation
pub struct FunCallsGpt<'a, W: Write, T: crate::dagops::DagOpsTrait> {
    chat_writer: FunCallsToChat<W>,
    dag_writer: crate::dagops::DagOpsWrite<'a, T>,
}

impl<'a, W: Write, T: crate::dagops::DagOpsTrait> FunCallsGpt<'a, W, T> {
    /// Create a new composite function call writer
    ///
    /// # Arguments
    /// * `writer` - Output writer for chat-style formatting
    /// * `dagops` - Mutable reference to DAG operations handler
    pub fn new(writer: W, dagops: &'a mut T) -> Self {
        Self {
            chat_writer: FunCallsToChat::new(writer),
            dag_writer: crate::dagops::DagOpsWrite::new(dagops),
        }
    }
}

impl<'a, W: Write, T: crate::dagops::DagOpsTrait> FunCallsWrite for FunCallsGpt<'a, W, T> {
    fn new_item(&mut self, id: &str, name: &str) -> FunCallResult {
        // Forward to both writers, failing fast on any error
        self.chat_writer.new_item(id, name)?;
        self.dag_writer.new_item(id, name)?;
        Ok(())
    }

    fn arguments_chunk(&mut self, chunk: &str) -> FunCallResult {
        self.chat_writer.arguments_chunk(chunk)?;
        self.dag_writer.arguments_chunk(chunk)?;
        Ok(())
    }

    fn end_item(&mut self) -> FunCallResult {
        self.chat_writer.end_item()?;
        self.dag_writer.end_item()?;
        Ok(())
    }

    fn end(&mut self) -> FunCallResult {
        self.chat_writer.end()?;
        self.dag_writer.end()?;
        Ok(())
    }
}
