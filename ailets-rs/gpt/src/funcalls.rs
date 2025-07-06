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
}

impl FunCalls {
    /// Creates a new empty collection of function calls
    #[must_use]
    pub fn new() -> Self {
        Self {
            idx: None,
            tool_calls: Vec::new(),
        }
    }

    fn get_cell(&mut self) -> Result<&mut ContentItemFunction, String> {
        let len = self.tool_calls.len();
        match self.idx {
            Some(idx) => self
                .tool_calls
                .get_mut(idx)
                .ok_or_else(|| format!("Delta index is out of bounds: {idx}, n of deltas: {len}")),
            None => Err("No active delta index".to_string()),
        }
    }

    /// Sets the current delta index and ensures space for the function call
    ///
    /// # Arguments
    /// * `index` - The index to set for the current delta position
    ///
    /// # Errors
    /// Returns an error if the index is invalid
    pub fn delta_index(&mut self, index: usize) -> Result<(), String> {
        if self.idx != Some(index) {
            self.idx = Some(index);
        }
        while self.tool_calls.len() <= index {
            self.tool_calls.push(ContentItemFunction::default());
        }
        Ok(())
    }

    /// Appends to the ID of the current function call
    ///
    /// # Arguments
    /// * `id` - String to append to the current function call's ID
    ///
    /// # Errors
    /// Returns an error if the current index is invalid
    pub fn delta_id(&mut self, id: &str) -> Result<(), String> {
        let cell = self.get_cell()?;
        cell.id.push_str(id);
        Ok(())
    }

    /// Appends to the function name of the current function call
    ///
    /// # Arguments
    /// * `function_name` - String to append to the current function call's name
    ///
    /// # Errors
    /// Returns an error if the current index is invalid
    pub fn delta_function_name(&mut self, function_name: &str) -> Result<(), String> {
        let cell = self.get_cell()?;
        cell.function_name.push_str(function_name);
        Ok(())
    }

    /// Appends to the function arguments of the current function call
    ///
    /// # Arguments
    /// * `function_arguments` - String to append to the current function call's arguments
    ///
    /// # Errors
    /// Returns an error if the current index is invalid
    pub fn delta_function_arguments(&mut self, function_arguments: &str) -> Result<(), String> {
        let cell = self.get_cell()?;
        cell.function_arguments.push_str(function_arguments);
        Ok(())
    }

    /// Returns a reference to the vector of function calls
    #[must_use]
    pub fn get_tool_calls(&self) -> &Vec<ContentItemFunction> {
        &self.tool_calls
    }
}
