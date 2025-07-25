//! Collect function calls from an AI model response
//!
//! - Tracking individual function calls with their IDs, names, and arguments
//! - Managing collections of function calls
//! - Incrementally building function calls through delta updates
//! - Writing function call data directly to output writers
//!
//! The primary structures are:
//! - [`FunCalls`]: Manages a collection of function calls with delta-based updates and direct writing
//! - [`FunCallsWrite`]: Trait for writing function calls to different outputs

/// Trait for writing function calls to different outputs
///
/// This trait allows different implementations of writing function calls,
/// enabling the `FunCalls` struct to be a validation driver while
/// delegating the actual writing to implementations of this trait.
pub trait FunCallsWrite {
    /// Start a new function call item
    ///
    /// # Arguments
    /// * `id` - The unique identifier for the function call
    /// * `name` - The name of the function to be called
    ///
    /// # Errors
    /// Returns error if the writing operation fails
    fn new_item(&mut self, id: String, name: String) -> Result<(), Box<dyn std::error::Error>>;

    /// Add a chunk of arguments to the current function call
    ///
    /// # Arguments
    /// * `ac` - The arguments chunk to add
    ///
    /// # Errors
    /// Returns error if the writing operation fails
    fn arguments_chunk(&mut self, ac: String) -> Result<(), Box<dyn std::error::Error>>;

    /// Finalize the current function call item
    ///
    /// # Errors
    /// Returns error if the writing operation fails
    fn end_item(&mut self) -> Result<(), Box<dyn std::error::Error>>;

    /// Finalize all function call processing
    ///
    /// # Errors
    /// Returns error if the writing operation fails
    fn end(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}

/// No-op implementation of `FunCallsWrite` for parsing/streaming mode
/// This writer does nothing - it's used when we only want to update `FunCalls` state
/// without actually writing anything.
pub struct NoOpFunCallsWrite;

impl FunCallsWrite for NoOpFunCallsWrite {
    fn new_item(&mut self, _id: String, _name: String) -> Result<(), Box<dyn std::error::Error>> {
        Ok(()) // Do nothing
    }

    fn arguments_chunk(&mut self, _ac: String) -> Result<(), Box<dyn std::error::Error>> {
        Ok(()) // Do nothing
    }

    fn end_item(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(()) // Do nothing
    }

    fn end(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(()) // Do nothing
    }
}

/// Implementation of `FunCallsWrite` that writes to a chat-style format
///
/// This implementation writes function calls in the format expected by chat systems,
/// with function call data written as JSON lines.
pub struct FunCallsToChat<W: std::io::Write> {
    writer: W,
    current_id: Option<String>,
    current_name: Option<String>,
    current_arguments: String,
}

impl<W: std::io::Write> FunCallsToChat<W> {
    /// Creates a new `FunCallsToChat` instance with the given writer
    #[must_use]
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            current_id: None,
            current_name: None,
            current_arguments: String::new(),
        }
    }
}

impl<W: std::io::Write> FunCallsWrite for FunCallsToChat<W> {
    fn new_item(&mut self, id: String, name: String) -> Result<(), Box<dyn std::error::Error>> {
        // Store the id and name for writing later
        self.current_id = Some(id);
        self.current_name = Some(name);
        self.current_arguments.clear();
        Ok(())
    }

    fn arguments_chunk(&mut self, ac: String) -> Result<(), Box<dyn std::error::Error>> {
        // Accumulate arguments chunks
        self.current_arguments.push_str(&ac);
        Ok(())
    }

    fn end_item(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Write the complete function call
        if let (Some(id), Some(name)) = (&self.current_id, &self.current_name) {
            writeln!(
                self.writer,
                r#"[{{"type":"tool_call"}},{{"id":"{}","function_name":"{}","function_arguments":"{}"}}]"#,
                id, name, self.current_arguments
            )?;
        }

        // Clear state for next item
        self.current_id = None;
        self.current_name = None;
        self.current_arguments.clear();
        Ok(())
    }

    fn end(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // FunCallsToChat doesn't need to do anything special on end
        Ok(())
    }
}

/// State of a function call being built
#[derive(Debug, Default, PartialEq, Eq, Clone)]
struct FunctionCallState {
    id: Option<String>,
    name: Option<String>,
    new_item_called: bool,
    pending_arguments: String,
}

impl FunctionCallState {
    fn new() -> Self {
        Self::default()
    }

    fn reset(&mut self) {
        *self = Self::new();
    }
}

/// A collection of function calls with support for incremental updates and validation
#[derive(Debug, Default, PartialEq, Eq, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct FunCalls {
    // Core delta/streaming state
    pub last_index: Option<usize>,

    // Direct writing state
    current_call: FunctionCallState,

    // Streaming-specific state
    last_streamed_index: Option<usize>,
    tool_call_open: bool,
    tool_call_arguments_open: bool,
    id_streamed: bool,
    name_streamed: bool,
}

impl FunCalls {
    /// Creates a new empty collection of function calls
    #[must_use]
    pub fn new() -> Self {
        Self {
            last_index: None,
            current_call: FunctionCallState::new(),
            last_streamed_index: None,
            tool_call_open: false,
            tool_call_arguments_open: false,
            id_streamed: false,
            name_streamed: false,
        }
    }

    /// Calls `new_item` immediately if both id and name are now available
    fn try_call_new_item(
        &mut self,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.current_call.new_item_called {
            if let (Some(id), Some(name)) = (&self.current_call.id, &self.current_call.name) {
                writer.new_item(id.clone(), name.clone())?;
                self.current_call.new_item_called = true;

                // Send any pending arguments that were accumulated before new_item
                if !self.current_call.pending_arguments.is_empty() {
                    writer.arguments_chunk(self.current_call.pending_arguments.clone())?;
                    self.current_call.pending_arguments.clear();
                }

                // Clear id and name - no longer needed after new_item
                self.current_call.id = None;
                self.current_call.name = None;
            }
        }
        Ok(())
    }

    /// Ends the current function call and cleans up the delta state (internal use)
    ///
    /// This method should be called when a function call is complete.
    pub fn end_current_internal(&mut self) {
        // current_funcall field has been removed
        self.id_streamed = false;
        self.name_streamed = false;
        self.current_call.reset();
    }

    /// Ends the current function call and writes it to the output
    ///
    /// # Errors
    /// Returns error if the writing operation fails
    pub fn end_current(
        &mut self,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Always call end_item since we track state differently now
        writer.end_item()?;
        // Reset state for next function call
        self.end_current_internal();
        Ok(())
    }

    /// Ends the current function call without writing (for delta mode)
    pub fn end_current_no_write(&mut self) {
        self.end_current_internal();
    }

    /// Ends the current item
    ///
    /// # Arguments
    /// * `writer` - The writer to use for ending the item
    ///
    /// # Errors
    /// Returns error if "`end_item`" is called without "`new_item`" being called first
    pub fn end_item(
        &mut self,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // If "end_item" is called without our logic called "new_item", it's an error
        if !self.current_call.new_item_called {
            return Err("end_item called without new_item being called first".into());
        }

        writer.end_item()?;
        // Reset the state since this call is now complete
        self.current_call.new_item_called = false;

        Ok(())
    }

    /// Reset streaming state (called when beginning a new message)
    pub fn reset_streaming_state(&mut self) {
        self.last_index = None;
        self.last_streamed_index = None;
        self.tool_call_open = false;
        self.tool_call_arguments_open = false;
        self.id_streamed = false;
        self.name_streamed = false;
        self.current_call.reset();
    }

    /// Check if arguments are currently being streamed
    #[must_use]
    pub fn is_streaming_arguments(&self) -> bool {
        self.tool_call_arguments_open
    }

    /// Mark the current streaming tool call as closed
    pub fn close_current_streaming_tool_call(&mut self) {
        self.tool_call_open = false;
        self.tool_call_arguments_open = false;
    }

    /// Get current arguments for streaming (returns what we have so far)
    #[must_use]
    pub fn get_current_arguments(&self) -> Option<String> {
        if self.current_call.pending_arguments.is_empty() {
            None
        } else {
            Some(self.current_call.pending_arguments.clone())
        }
    }

    // Direct writing methods for immediate output

    /// Sets the index and starts a new tool call if necessary
    ///
    /// # Errors
    /// Returns error if validation fails or writing operation fails
    pub fn index(
        &mut self,
        index: usize,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Validate streaming assumption: index progression
        match self.last_index {
            None => {
                // First index must be 0
                if index != 0 {
                    return Err(format!("First tool call index must be 0, got {index}").into());
                }
            }
            Some(last) => {
                // Index can stay the same or increment by exactly 1, but never decrease
                if index < last {
                    return Err(format!(
                        "Tool call index cannot decrease, max seen is {last}, got {index}"
                    )
                    .into());
                }
                if index > last + 1 {
                    return Err(format!(
                        "Tool call index cannot skip values, max seen is {last}, got {index}"
                    )
                    .into());
                }

                // If we're moving to a new index, end the current function call and call end_item if needed
                if index > last {
                    if self.current_call.new_item_called {
                        writer.end_item()?;
                    }
                    self.end_current_internal();
                }
            }
        }

        // Update last_index to track the highest seen index (enables streaming mode)
        self.last_index = Some(index);
        Ok(())
    }

    /// Sets the ID of the current function call
    ///
    /// # Errors
    /// Returns error if validation fails
    pub fn id(
        &mut self,
        id: &str,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if ID is already set or new_item already called
        if self.current_call.new_item_called || self.current_call.id.is_some() {
            return Err("ID is already given".into());
        }

        // Store the ID
        self.current_call.id = Some(id.to_string());

        // ID is now stored only in current_call

        // Call new_item immediately if both id and name are now available
        self.try_call_new_item(writer)?;

        Ok(())
    }

    /// Sets the name of the current function call
    ///
    /// # Errors
    /// Returns error if validation fails or writing operation fails
    pub fn name(
        &mut self,
        name: &str,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if name is already set or new_item already called
        if self.current_call.new_item_called || self.current_call.name.is_some() {
            return Err("Name is already given".into());
        }

        // Store the name
        self.current_call.name = Some(name.to_string());

        // Name is now stored only in current_call

        // Call new_item immediately if both id and name are now available
        self.try_call_new_item(writer)?;

        Ok(())
    }

    /// Adds arguments to the current function call
    ///
    /// # Errors
    /// Returns error if writing operation fails
    pub fn arguments_chunk(
        &mut self,
        args: &str,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Arguments are now stored only in current_call

        // Pass arguments directly to writer after new_item has been called
        if self.current_call.new_item_called {
            writer.arguments_chunk(args.to_string())?;
        } else {
            // Store arguments until new_item is called
            self.current_call.pending_arguments.push_str(args);
        }

        Ok(())
    }

    /// Finalizes all function calls
    ///
    /// # Errors
    /// Returns error if writing operation fails
    pub fn end(
        &mut self,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // "end" calls "end_item" if "end_item" was not called
        if self.current_call.new_item_called {
            writer.end_item()?;
            self.current_call.new_item_called = false;
        }
        Ok(())
    }
}

/// `FunCallsGpt` forwards function call events to both `FunCallsToChat` and `DagOpsWrite`
pub struct FunCallsGpt<'a, W: std::io::Write, T: crate::dagops::DagOpsTrait> {
    chat_writer: crate::funcalls::FunCallsToChat<W>,
    dag_writer: crate::dagops::DagOpsWrite<'a, T>,
}

impl<'a, W: std::io::Write, T: crate::dagops::DagOpsTrait> FunCallsGpt<'a, W, T> {
    /// Create a new `FunCallsGpt` instance
    pub fn new(writer: W, dagops: &'a mut T) -> Self {
        Self {
            chat_writer: crate::funcalls::FunCallsToChat::new(writer),
            dag_writer: crate::dagops::DagOpsWrite::new(dagops),
        }
    }
}

impl<'a, W: std::io::Write, T: crate::dagops::DagOpsTrait> FunCallsWrite for FunCallsGpt<'a, W, T> {
    fn new_item(&mut self, id: String, name: String) -> Result<(), Box<dyn std::error::Error>> {
        self.chat_writer.new_item(id.clone(), name.clone())?;
        self.dag_writer.new_item(id, name)?;
        Ok(())
    }

    fn arguments_chunk(&mut self, args: String) -> Result<(), Box<dyn std::error::Error>> {
        self.chat_writer.arguments_chunk(args.clone())?;
        self.dag_writer.arguments_chunk(args)?;
        Ok(())
    }

    fn end_item(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.chat_writer.end_item()?;
        self.dag_writer.end_item()?;
        Ok(())
    }

    fn end(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.chat_writer.end()?;
        self.dag_writer.end()?;
        Ok(())
    }
}
