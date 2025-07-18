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
    idx: Option<usize>,
    tool_calls: Vec<ContentItemFunction>,
    current_funcall: Option<ContentItemFunction>,
}

impl FunCalls {
    /// Creates a new empty collection of function calls
    #[must_use]
    pub fn new() -> Self {
        Self {
            idx: None,
            tool_calls: Vec::new(),
            current_funcall: None,
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
            if let Some(idx) = self.idx {
                // Streaming mode: replace the element at the index
                if let Some(tool_call) = self.tool_calls.get_mut(idx) {
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

        // Reset the streaming mode index
        self.idx = None;
    }

    /// Sets the current delta index and ensures space for the function call
    ///
    /// This method:
    /// - Ensures there's enough space in the vector for the given index
    /// - Merges any existing `current_funcall` data with the vector entry at the specified index
    /// - Sets the streaming mode index
    /// - Initializes `current_funcall` with the existing data at the specified index
    ///
    /// # Arguments
    /// * `index` - The index to set for the current delta position
    pub fn delta_index(&mut self, index: usize) {
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

        // Set the streaming mode index
        self.idx = Some(index);

        // Initialize current_funcall with the existing data at the specified index
        if self.current_funcall.is_none() {
            if let Some(existing_funcall) = self.tool_calls.get(index) {
                self.current_funcall = Some(ContentItemFunction {
                    id: existing_funcall.id.clone(),
                    function_name: existing_funcall.function_name.clone(),
                    function_arguments: existing_funcall.function_arguments.clone(),
                });
            }
        }
    }

    /// Appends to the ID of the current function call
    ///
    /// # Arguments
    /// * `id` - String to append to the current function call's ID
    ///
    pub fn delta_id(&mut self, id: &str) {
        let cell = self.ensure_current();
        cell.id.push_str(id);
    }

    /// Appends to the function name of the current function call
    ///
    /// # Arguments
    /// * `function_name` - String to append to the current function call's name
    ///
    pub fn delta_function_name(&mut self, function_name: &str) {
        let cell = self.ensure_current();
        cell.function_name.push_str(function_name);
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
}
