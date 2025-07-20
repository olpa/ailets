//! Collect function calls from an AI model response
//!
//! - Tracking individual function calls with their IDs, names, and arguments
//! - Managing collections of function calls
//! - Incrementally building function calls through delta updates
//!
//! The primary structures are:
//! - [`ContentItemFunction`]: Represents a single function call with its metadata
//! - [`FunCalls`]: Manages a collection of function calls with delta-based updates

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

/// A collection of function calls with support for incremental updates
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct FunCalls {
    current_funcall: Option<ContentItemFunction>,
    last_index: Option<usize>,
    // Streaming-related fields moved from StructureBuilder
    streaming_mode: bool,
    last_streamed_index: Option<usize>,
    tool_call_open: bool,
    tool_call_arguments_open: bool,
}

impl FunCalls {
    /// Creates a new empty collection of function calls
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_funcall: None,
            last_index: None,
            streaming_mode: false,
            last_streamed_index: None,
            tool_call_open: false,
            tool_call_arguments_open: false,
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

    /// Ends the current function call and cleans up the delta state
    ///
    /// This method should be called when a function call is complete.
    pub fn end_current(&mut self) {
        self.current_funcall = None;
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
                    self.end_current();
                }
            }
        }

        // Enable streaming mode and update last_index to track the highest seen index
        self.streaming_mode = true;
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
        if self.streaming_mode {
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
        if self.streaming_mode {
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

    /// Returns a reference to the current function call being built
    #[must_use]
    pub fn get_current_funcall(&self) -> &Option<ContentItemFunction> {
        &self.current_funcall
    }

    /// Reset streaming state (called when beginning a new message)
    pub fn reset_streaming_state(&mut self) {
        self.streaming_mode = false;
        self.last_streamed_index = None;
        self.tool_call_open = false;
        self.tool_call_arguments_open = false;
    }

    /// Get the current completed tool call if it's ready to be streamed
    /// Returns the current tool call if it's complete and hasn't been streamed yet
    pub fn get_completed_tool_call_for_streaming(&mut self) -> Option<ContentItemFunction> {
        if !self.streaming_mode {
            return None;
        }

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
        if !self.streaming_mode || self.tool_call_open {
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
}
