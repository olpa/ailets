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
                r#"[{{"type":"function","id":"{}","name":"{}"}},{{"arguments":"{}"}}]"#,
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

/// A collection of function calls with support for incremental updates and validation
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct FunCalls {
    current_funcall: Option<ContentItemFunction>,
    last_index: Option<usize>,
    last_streamed_index: Option<usize>,
    tool_call_open: bool,
    tool_call_arguments_open: bool,
    id_streamed: bool,
    name_streamed: bool,
    // New fields for the refactored logic
    current_id: Option<String>,
    current_name: Option<String>,
    stored_arguments_chunk: Option<String>,
    new_item_called: bool,
    end_item_called: bool,
}

impl FunCalls {
    /// Creates a new empty collection of function calls
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_funcall: None,
            last_index: None,
            last_streamed_index: None,
            tool_call_open: false,
            tool_call_arguments_open: false,
            id_streamed: false,
            name_streamed: false,
            current_id: None,
            current_name: None,
            stored_arguments_chunk: None,
            new_item_called: false,
            end_item_called: false,
        }
    }

    /// Ensures the current function call is initialized
    ///
    /// Returns a mutable reference to the current function call,
    /// initializing it with a default `ContentItemFunction` if it wasn't already set.
    #[must_use]
    fn ensure_current(&mut self) -> &mut ContentItemFunction {
        self.current_funcall
            .get_or_insert_with(ContentItemFunction::default)
    }

    /// Ends the current function call and cleans up the delta state (internal use)
    ///
    /// This method should be called when a function call is complete.
    pub fn end_current_internal(&mut self) {
        self.current_funcall = None;
        self.id_streamed = false;
        self.name_streamed = false;
        self.current_id = None;
        self.current_name = None;
        self.stored_arguments_chunk = None;
        self.new_item_called = false;
        self.end_item_called = false;
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

    /// Sets the current delta index for streaming mode
    ///
    /// This method:
    /// - Validates streaming assumptions (index increments properly)
    /// - Enables streaming mode and updates the last seen index
    /// - When switching to a new index, ends the current function call
    ///
    /// # Arguments
    /// * `index` - The index to set for the current delta position
    ///
    /// # Errors
    /// Returns error if streaming assumptions are violated
    pub fn delta_index(&mut self, index: usize) -> Result<(), String> {
        // Validate streaming assumption: index progression
        match self.last_index {
            None => {
                // First index must be 0
                if index != 0 {
                    return Err(format!("First tool call index must be 0, got {index}"));
                }
            }
            Some(last) => {
                // Index can stay the same or increment by exactly 1, but never decrease
                if index < last {
                    return Err(format!(
                        "Tool call index cannot decrease, max seen is {last}, got {index}"
                    ));
                }
                if index > last + 1 {
                    return Err(format!(
                        "Tool call index cannot skip values, max seen is {last}, got {index}"
                    ));
                }
                // If we're moving to a new index, end the current function call
                if index > last {
                    self.end_current_internal();
                }
            }
        }

        // Update last_index to track the highest seen index (enables streaming mode)
        self.last_index = Some(index);

        Ok(())
    }

    /// Appends to the ID of the current function call
    ///
    /// # Arguments
    /// * `id` - String to append to the current function call's ID
    ///
    /// # Errors
    /// Returns error if streaming assumptions are violated (ID set multiple times in streaming mode)
    pub fn delta_id(&mut self, id: &str) -> Result<(), String> {
        // Check streaming assumption: in streaming mode, non-argument fields should only be set once
        if self.last_index.is_some() {
            if let Some(current) = &self.current_funcall {
                if !current.id.is_empty() {
                    return Err("ID field cannot be set multiple times in streaming mode - only arguments can span deltas".to_string());
                }
            }
        }

        let cell = self.ensure_current();
        cell.id.push_str(id);

        Ok(())
    }

    /// Appends to the function name of the current function call
    ///
    /// # Arguments
    /// * `function_name` - String to append to the current function call's name
    ///
    /// # Errors
    /// Returns error if streaming assumptions are violated (name set multiple times in streaming mode)
    pub fn delta_function_name(&mut self, function_name: &str) -> Result<(), String> {
        // Check streaming assumption: in streaming mode, non-argument fields should only be set once
        if self.last_index.is_some() {
            if let Some(current) = &self.current_funcall {
                if !current.function_name.is_empty() {
                    return Err("Function name field cannot be set multiple times in streaming mode - only arguments can span deltas".to_string());
                }
            }
        }

        let cell = self.ensure_current();
        cell.function_name.push_str(function_name);

        Ok(())
    }

    /// Appends to the function arguments of the current function call
    ///
    /// # Arguments
    /// * `function_arguments` - String to append to the current function call's arguments
    ///
    pub fn delta_function_arguments(&mut self, function_arguments: &str) {
        let cell = self.ensure_current();
        cell.function_arguments.push_str(function_arguments);
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
        if !self.new_item_called {
            return Err("end_item called without new_item being called first".into());
        }

        writer.end_item()?;
        self.end_item_called = true;

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
        self.current_id = None;
        self.current_name = None;
        self.stored_arguments_chunk = None;
        self.new_item_called = false;
        self.end_item_called = false;
    }

    /// Get the current completed tool call if it's ready to be streamed
    /// Returns the current tool call if it's complete and hasn't been streamed yet
    pub fn get_completed_tool_call_for_streaming(&mut self) -> Option<ContentItemFunction> {
        self.last_index?;

        // Check if we have a current function call that's complete
        if let Some(current) = &self.current_funcall {
            // A tool call is complete if all required fields are present
            // Arguments can be empty ("") but id and function_name must be non-empty
            if !current.id.is_empty()
                && !current.function_name.is_empty()
                && !current.function_arguments.is_empty()
            {
                // Check if this is a new completed call (hasn't been streamed yet)
                if let Some(last_index) = self.last_index {
                    if self
                        .last_streamed_index
                        .map_or(true, |streamed| last_index > streamed)
                    {
                        self.last_streamed_index = Some(last_index);
                        return Some(current.clone());
                    }
                }
            }
        }

        None
    }

    /// Check if we should start streaming the current tool call (id and name are ready)
    /// Returns the current tool call if it's ready to start streaming
    pub fn get_current_tool_call_for_streaming(&mut self) -> Option<ContentItemFunction> {
        if self.last_index.is_none() || self.tool_call_open {
            return None;
        }

        // Get current tool call data
        if let Some(current_funcall) = &self.current_funcall {
            if !current_funcall.id.is_empty() && !current_funcall.function_name.is_empty() {
                self.tool_call_open = true;
                self.tool_call_arguments_open = true;
                return Some(current_funcall.clone());
            }
        }
        None
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

    /// Check if the ID is ready to be streamed and hasn't been streamed yet
    pub fn should_stream_id(&mut self) -> Option<String> {
        if self.id_streamed {
            return None;
        }

        if let Some(current) = &self.current_funcall {
            if !current.id.is_empty() {
                self.id_streamed = true;
                return Some(current.id.clone());
            }
        }
        None
    }

    /// Check if the name is ready to be streamed and hasn't been streamed yet
    pub fn should_stream_name(&mut self) -> Option<String> {
        if self.name_streamed {
            return None;
        }

        if let Some(current) = &self.current_funcall {
            if !current.function_name.is_empty() {
                self.name_streamed = true;
                return Some(current.function_name.clone());
            }
        }
        None
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
        // If we're moving to a new index, call end_item if it wasn't called
        if let Some(last_index) = self.last_index {
            if index > last_index && !self.end_item_called && self.new_item_called {
                writer.end_item()?;
                self.end_item_called = true;
            }
        }

        self.delta_index(index).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as Box<dyn std::error::Error>
        })?;
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
        // If "id" is already given, error
        if self.current_id.is_some() {
            return Err("ID is already given".into());
        }

        // Store "id"
        self.current_id = Some(id.to_string());

        // If "name" is already given
        if let Some(name) = &self.current_name {
            // Call "FunCallsWrite.new_item(index, id, name)"
            let current_index = self.last_index.unwrap_or(0);
            writer.new_item(current_index, id.to_string(), name.clone())?;
            self.new_item_called = true;

            // If arguments_chunk is stored, pass it to arguments_chunk
            if let Some(args_chunk) = &self.stored_arguments_chunk {
                writer.arguments_chunk(args_chunk.clone())?;
                self.stored_arguments_chunk = None;
            }
        }

        // Also update the delta system for backward compatibility
        self.delta_id(id).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as Box<dyn std::error::Error>
        })?;
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
        // If "name" is already given, error
        if self.current_name.is_some() {
            return Err("Name is already given".into());
        }

        // Store "name"
        self.current_name = Some(name.to_string());

        // If "id" is already given
        if let Some(id) = &self.current_id {
            // Call "FunCallsWrite.new_item(index, id, name)"
            let current_index = self.last_index.unwrap_or(0);
            writer.new_item(current_index, id.clone(), name.to_string())?;
            self.new_item_called = true;

            // If arguments_chunk is stored, pass it to arguments_chunk
            if let Some(args_chunk) = &self.stored_arguments_chunk {
                writer.arguments_chunk(args_chunk.clone())?;
                self.stored_arguments_chunk = None;
            }
        }

        // Also update the delta system for backward compatibility
        self.delta_function_name(name).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                as Box<dyn std::error::Error>
        })?;
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
        // If "FunCalls.new_item" was called, call "writer.arguments_chunk"
        if self.new_item_called {
            writer.arguments_chunk(args.to_string())?;
        } else {
            // Otherwise, store it
            if let Some(stored) = &mut self.stored_arguments_chunk {
                stored.push_str(args);
            } else {
                self.stored_arguments_chunk = Some(args.to_string());
            }
        }

        // Also update the delta system for backward compatibility
        self.delta_function_arguments(args);
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
        if !self.end_item_called && self.new_item_called {
            writer.end_item()?;
            self.end_item_called = true;
        }
        Ok(())
    }
}
