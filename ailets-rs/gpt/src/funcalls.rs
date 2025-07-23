//! Collect function calls from an AI model response
//!
//! - Tracking individual function calls with their IDs, names, and arguments
//! - Managing collections of function calls
//! - Incrementally building function calls through delta updates
//! - Writing function call data directly to output writers
//!
//! The primary structures are:
//! - [`ContentItemFunction`]: Represents a single function call with its metadata
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
    /// * `index` - The index of the function call
    /// * `id` - The unique identifier for the function call
    /// * `name` - The name of the function to be called
    ///
    /// # Errors
    /// Returns error if the writing operation fails
    fn new_item(
        &mut self,
        index: usize,
        id: String,
        name: String,
    ) -> Result<(), Box<dyn std::error::Error>>;

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
}

/// No-op implementation of `FunCallsWrite` for parsing/streaming mode
/// This writer does nothing - it's used when we only want to update FunCalls state
/// without actually writing anything.
pub struct NoOpFunCallsWrite;

impl FunCallsWrite for NoOpFunCallsWrite {
    fn new_item(
        &mut self,
        _index: usize,
        _id: String,
        _name: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(()) // Do nothing
    }

    fn arguments_chunk(&mut self, _ac: String) -> Result<(), Box<dyn std::error::Error>> {
        Ok(()) // Do nothing
    }

    fn end_item(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(()) // Do nothing
    }
}

/// Implementation of `FunCallsWrite` that writes to a chat-style format
///
/// This implementation writes function calls in the format expected by chat systems,
/// with control messages and function call data written as JSON lines.
pub struct FunCallsToChat<W: std::io::Write> {
    writer: W,
    first_call: bool,
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
            first_call: true,
            current_id: None,
            current_name: None,
            current_arguments: String::new(),
        }
    }

    /// Creates a new `FunCallsToChat` instance that won't write the control message
    /// (assumes the control message was already written)
    #[must_use]
    pub fn new_no_ctl(writer: W) -> Self {
        Self {
            writer,
            first_call: false,
            current_id: None,
            current_name: None,
            current_arguments: String::new(),
        }
    }
}

impl<W: std::io::Write> FunCallsWrite for FunCallsToChat<W> {
    fn new_item(
        &mut self,
        _index: usize,
        id: String,
        name: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.first_call {
            writeln!(self.writer, r#"[{{"type":"ctl"}},{{"role":"assistant"}}]"#)?;
            self.first_call = false;
        }

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
}

/// Represents a single function/tool call from an AI model response
///
/// Contains the essential metadata for a function call:
/// - A unique identifier
/// - The name of the function to be called
/// - The arguments to pass to the function (as a JSON string)
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct ContentItemFunction {
    // type: "function",
    pub id: String,
    pub function_name: String,
    pub function_arguments: String,
}

impl ContentItemFunction {
    /// Creates a new function call
    #[must_use]
    pub fn new(id: &str, function_name: &str, function_arguments: &str) -> Self {
        Self {
            id: id.to_string(),
            function_name: function_name.to_string(),
            function_arguments: function_arguments.to_string(),
        }
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
pub struct FunCalls {
    // Core delta/streaming state
    pub current_funcall: Option<ContentItemFunction>,
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
            current_funcall: None,
            last_index: None,
            current_call: FunctionCallState::new(),
            last_streamed_index: None,
            tool_call_open: false,
            tool_call_arguments_open: false,
            id_streamed: false,
            name_streamed: false,
        }
    }

    /// Ensures the current function call is initialized
    ///
    /// Returns a mutable reference to the current function call,
    /// initializing it with a default `ContentItemFunction` if it wasn't already set.
    #[must_use]
    pub fn ensure_current(&mut self) -> &mut ContentItemFunction {
        self.current_funcall
            .get_or_insert_with(ContentItemFunction::default)
    }

    /// Calls new_item immediately if both id and name are now available
    fn try_call_new_item(
        &mut self,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.current_call.new_item_called
            && self.current_call.id.is_some()
            && self.current_call.name.is_some()
        {
            let current_index = self.last_index.unwrap_or(0);
            writer.new_item(
                current_index,
                self.current_call.id.as_ref().unwrap().clone(),
                self.current_call.name.as_ref().unwrap().clone(),
            )?;
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
        Ok(())
    }

    /// Ends the current function call and cleans up the delta state (internal use)
    ///
    /// This method should be called when a function call is complete.
    pub fn end_current_internal(&mut self) {
        self.current_funcall = None;
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
        if let Some(_current) = &self.current_funcall {
            writer.end_item()?;
        }
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
    /// Returns error if "end_item" is called without "new_item" being called first
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

    /// Returns a reference to the current function call being built
    #[must_use]
    pub fn get_current_funcall(&self) -> &Option<ContentItemFunction> {
        &self.current_funcall
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
        if let Some(current) = &self.current_funcall {
            if !current.function_arguments.is_empty() {
                return Some(current.function_arguments.clone());
            }
        }
        None
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

        // Also update the streaming state for compatibility
        let cell = self.ensure_current();
        cell.id.push_str(id);

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

        // Also update the streaming state for compatibility
        let cell = self.ensure_current();
        cell.function_name.push_str(name);

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
        // Update the streaming state for compatibility
        let cell = self.ensure_current();
        cell.function_arguments.push_str(args);

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
