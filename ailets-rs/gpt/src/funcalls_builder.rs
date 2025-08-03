//! Function call state management for streaming processing
//!
//! This module provides state management for function calls during streaming
//! processing, ensuring proper sequencing and validation of function call data.

use crate::fcw_trait::FunCallsWrite;

/// State manager for streaming function call processing
///
/// Manages function call state during streaming processing, ensuring:
/// - Proper sequencing of function calls by index
/// - Correct pairing of ID and name before processing
/// - Buffering of arguments until the function call is ready
/// - State transitions follow the expected protocol
#[derive(Debug)]
pub struct FunCallsBuilder {
    /// The highest function call index seen so far (enables streaming mode)
    pub last_index: Option<usize>,
    /// Current function call ID (waiting for name to complete setup)
    current_id: Option<String>,
    /// Current function call name (waiting for ID to complete setup)
    current_name: Option<String>,
    /// Arguments accumulated before `new_item` was called
    pending_arguments: Option<Vec<u8>>,
    /// Whether `new_item` has been called for the current function call
    new_item_called: bool,
}

impl FunCallsBuilder {
    /// Creates a new function call state manager
    ///
    /// # Returns
    /// A new `FunCallsBuilder` instance ready to process streaming function calls
    #[must_use]
    pub fn new() -> Self {
        Self {
            last_index: None,
            current_id: None,
            current_name: None,
            pending_arguments: None,
            new_item_called: false,
        }
    }

    // =========================================================================
    // Private Helper Methods
    // =========================================================================

    /// Attempts to call `new_item` if both ID and name are available
    ///
    /// This method handles the coordination between ID and name arrival,
    /// calling the writers' `new_item` methods when both are present.
    ///
    /// # Arguments
    /// * `chat_writer` - The chat writer to call `new_item` on
    /// * `dag_writer` - The dag writer to call `new_item` on
    ///
    /// # Errors
    /// Returns an error if the writers' `new_item` or `arguments_chunk` methods fail
    fn try_call_new_item(
        &mut self,
        chat_writer: &mut dyn FunCallsWrite,
        dag_writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.new_item_called {
            if let (Some(id), Some(name)) = (&self.current_id, &self.current_name) {
                chat_writer.new_item(id, name)?;
                dag_writer.new_item(id, name)?;
                self.new_item_called = true;

                // Send any pending arguments that were accumulated before new_item
                if let Some(ref args) = self.pending_arguments {
                    chat_writer.arguments_chunk(args)?;
                    dag_writer.arguments_chunk(args)?;
                    self.pending_arguments = None;
                }

                // Clear id and name - no longer needed after new_item
                self.current_id = None;
                self.current_name = None;
            }
        }
        Ok(())
    }

    // =========================================================================
    // Public Interface Methods
    // =========================================================================

    /// Ends the current function call and writes it to the output
    ///
    /// This method finalizes the current function call and resets state
    /// for the next function call.
    ///
    /// # Arguments
    /// * `chat_writer` - The chat writer to finalize the function call with
    /// * `dag_writer` - The dag writer to finalize the function call with
    ///
    /// # Errors
    /// Returns error if the writing operation fails
    pub fn end_current(
        &mut self,
        chat_writer: &mut dyn FunCallsWrite,
        dag_writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Always call end_item since we track state differently now
        chat_writer.end_item()?;
        dag_writer.end_item()?;
        // Reset state for next function call
        self.current_id = None;
        self.current_name = None;
        self.new_item_called = false;
        self.pending_arguments = None;
        Ok(())
    }

    /// Ends the current item
    ///
    /// # Arguments
    /// * `chat_writer` - The chat writer to use for ending the item
    /// * `dag_writer` - The dag writer to use for ending the item
    ///
    /// # Errors
    /// Returns error if "`end_item`" is called without "`new_item`" being called first
    pub fn end_item(
        &mut self,
        chat_writer: &mut dyn FunCallsWrite,
        dag_writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.new_item_called {
            return Err("end_item called without new_item being called first".into());
        }

        chat_writer.end_item()?;
        dag_writer.end_item()?;
        self.new_item_called = false;

        Ok(())
    }

    /// Sets the index and starts a new tool call if necessary
    ///
    /// # Errors
    /// Returns error if validation fails or writing operation fails
    pub fn index(
        &mut self,
        index: usize,
        chat_writer: &mut dyn FunCallsWrite,
        dag_writer: &mut dyn FunCallsWrite,
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

                // If we're moving to a new index, end the current function call
                if index > last {
                    if self.new_item_called {
                        chat_writer.end_item()?;
                        dag_writer.end_item()?;
                    }
                    self.reset_current_call_state();
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
        chat_writer: &mut dyn FunCallsWrite,
        dag_writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if ID is already set or new_item already called
        if self.new_item_called || self.current_id.is_some() {
            return Err("ID is already given".into());
        }

        // Store the ID
        self.current_id = Some(id.to_string());

        // ID is now stored only in current_call

        // Call new_item immediately if both id and name are now available
        self.try_call_new_item(chat_writer, dag_writer)?;

        Ok(())
    }

    /// Sets the name of the current function call
    ///
    /// # Errors
    /// Returns error if validation fails or writing operation fails
    pub fn name(
        &mut self,
        name: &str,
        chat_writer: &mut dyn FunCallsWrite,
        dag_writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if name is already set or new_item already called
        if self.new_item_called || self.current_name.is_some() {
            return Err("Name is already given".into());
        }

        // Store the name
        self.current_name = Some(name.to_string());

        // Name is now stored only in current_call

        // Call new_item immediately if both id and name are now available
        self.try_call_new_item(chat_writer, dag_writer)?;

        Ok(())
    }

    /// Adds arguments to the current function call
    ///
    /// # Errors
    /// Returns error if writing operation fails
    pub fn arguments_chunk(
        &mut self,
        args: &[u8],
        chat_writer: &mut dyn FunCallsWrite,
        dag_writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Arguments are now stored only in current_call

        // Pass arguments directly to writers after new_item has been called
        if self.new_item_called {
            chat_writer.arguments_chunk(args)?;
            dag_writer.arguments_chunk(args)?;
        } else {
            // Store arguments until new_item is called
            match &mut self.pending_arguments {
                Some(existing) => existing.extend_from_slice(args),
                None => self.pending_arguments = Some(args.to_vec()),
            }
        }

        Ok(())
    }

    /// Finalizes all function calls
    ///
    /// # Errors
    /// Returns error if writing operation fails
    pub fn end(
        &mut self,
        chat_writer: &mut dyn FunCallsWrite,
        dag_writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // "end" calls "end_item" if "end_item" was not called
        if self.new_item_called {
            chat_writer.end_item()?;
            dag_writer.end_item()?;
            self.new_item_called = false;
        }
        Ok(())
    }

    // =========================================================================
    // Private Utility Methods
    // =========================================================================

    /// Resets the state for the current function call
    fn reset_current_call_state(&mut self) {
        self.current_id = None;
        self.current_name = None;
        self.new_item_called = false;
        self.pending_arguments = None;
    }
}

impl Default for FunCallsBuilder {
    fn default() -> Self {
        Self::new()
    }
}
