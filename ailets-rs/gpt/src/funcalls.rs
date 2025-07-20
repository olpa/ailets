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
    tool_calls: Vec<ContentItemFunction>,
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
            tool_calls: Vec::new(),
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
    /// In direct mode (no index), it pushes the current function call.
    /// In streaming mode (with index), it updates the function call at the current index.
    pub fn end_current(&mut self) {
        if let Some(current_funcall) = self.current_funcall.take() {
            if let Some(last_idx) = self.last_index {
                // Streaming mode: replace the element at the last index
                if let Some(tool_call) = self.tool_calls.get_mut(last_idx) {
                    *tool_call = current_funcall;
                } else {
                    // This shouldn't happen in normal operation, but handle gracefully
                    self.tool_calls.push(current_funcall);
                }
            } else {
                // Direct mode: push to the vector
                self.tool_calls.push(current_funcall);
            }
        }
    }

    /// Sets the current delta index and ensures space for the function call
    ///
    /// This method:
    /// - Validates streaming assumptions (index increments properly)
    /// - Ensures there's enough space in the vector for the given index
    /// - Merges any existing `current_funcall` data with the vector entry at the specified index
    /// - Enables streaming mode and updates the last seen index
    /// - Initializes `current_funcall` with the existing data at the specified index
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
            }
        }
        // Ensure we have enough space in the vector
        while self.tool_calls.len() <= index {
            self.tool_calls.push(ContentItemFunction::default());
        }

        // If we have a current function call in direct mode, merge it with the existing one at index
        if let Some(current_funcall) = self.current_funcall.take() {
            // Index is guaranteed to be valid after the while loop above
            if let Some(existing_funcall) = self.tool_calls.get_mut(index) {
                existing_funcall.id.push_str(&current_funcall.id);
                existing_funcall
                    .function_name
                    .push_str(&current_funcall.function_name);
                existing_funcall
                    .function_arguments
                    .push_str(&current_funcall.function_arguments);
            }
        }

        // Enable streaming mode and update last_index to track the highest seen index
        self.streaming_mode = true;
        self.last_index = Some(index);

        // Initialize current_funcall with the updated data at the specified index
        if let Some(existing_funcall) = self.tool_calls.get(index) {
            self.current_funcall = Some(ContentItemFunction {
                id: existing_funcall.id.clone(),
                function_name: existing_funcall.function_name.clone(),
                function_arguments: existing_funcall.function_arguments.clone(),
            });
        }

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

    /// Returns a reference to the vector of function calls
    #[must_use]
    pub fn get_tool_calls(&self) -> &Vec<ContentItemFunction> {
        &self.tool_calls
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

    /// Get completed tool calls that are ready to be streamed
    /// Returns a vector of (`index`, `tool_call`) pairs for completed tool calls
    /// that haven't been streamed yet, and updates internal streaming state
    pub fn get_completed_tool_calls_for_streaming(&mut self) -> Vec<(usize, ContentItemFunction)> {
        if !self.streaming_mode {
            return Vec::new();
        }

        let mut completed_calls = Vec::new();
        let start_index = self.last_streamed_index.map_or(0, |i| i + 1);

        // Check for completed tool calls that can be streamed
        for (index, tool_call) in self.tool_calls.iter().enumerate().skip(start_index) {
            // A tool call is complete if all required fields are present
            // Arguments can be empty ("") but id and function_name must be non-empty
            if !tool_call.id.is_empty() && !tool_call.function_name.is_empty() {
                // For streaming, we consider it complete when we have a basic structure
                // Arguments might still be streaming in, but we can output when they're done
                // This means arguments being non-empty OR having actual content
                if !tool_call.function_arguments.is_empty() {
                    completed_calls.push((index, tool_call.clone()));
                    self.last_streamed_index = Some(index);
                }
            } else {
                // Stop at first incomplete tool call (streaming order)
                break;
            }
        }

        completed_calls
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
