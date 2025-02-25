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
    id: String,
    function_name: String,
    function_arguments: String,
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
    idx: usize,
    tool_calls: Vec<ContentItemFunction>,
}

impl FunCalls {
    /// Creates a new empty collection of function calls
    #[must_use]
    pub fn new() -> Self {
        Self {
            idx: 0,
            tool_calls: Vec::new(),
        }
    }

    fn get_cell(&mut self) -> Result<&mut ContentItemFunction, String> {
        let len = self.tool_calls.len();
        let cell = self.tool_calls.get_mut(self.idx);
        match cell {
            Some(cell) => Ok(cell),
            None => Err(format!(
                "Delta index is out of bounds: {}, n of deltas: {}",
                self.idx, len
            )),
        }
    }

    /// Initiates a new round of delta updates by resetting the index
    pub fn start_delta_round(&mut self) {
        self.idx = usize::MAX;
    }

    /// Starts a new delta update by incrementing the index and ensuring space
    /// for the new function call
    pub fn start_delta(&mut self) {
        self.idx = if self.idx == usize::MAX {
            0
        } else {
            self.idx + 1
        };
        if self.idx >= self.tool_calls.len() {
            self.tool_calls.push(ContentItemFunction::default());
        }
    }

    /// Verifies that the provided index matches the current delta position
    ///
    /// # Arguments
    /// * `index` - Expected current position in the collection
    ///
    /// # Errors
    /// Returns an error if the provided index doesn't match the current position
    pub fn delta_index(&mut self, index: usize) -> Result<(), String> {
        if self.idx == index {
            return Ok(());
        }
        Err(format!(
            "Delta index mismatch. Got: {}, expected: {}",
            index, self.idx
        ))
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
