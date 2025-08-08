//! Function call state management for streaming processing
//!
//! This module provides state management for function calls during streaming
//! processing, ensuring proper sequencing and validation of function call data.

use crate::dagops::DagOpsTrait;
use crate::fcw_chat::FunCallsToChat;
use crate::fcw_tools::FunCallsToTools;
use crate::fcw_trait::FunCallsWrite;
use actor_io::AWriter;

/// State manager for streaming function call processing
///
/// Manages function call state during streaming processing, ensuring:
/// - Proper sequencing of function calls by index
/// - Correct pairing of ID and name before processing
/// - Buffering of arguments until the function call is ready
/// - State transitions follow the expected protocol
pub struct FunCallsBuilder<D: DagOpsTrait> {
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
    /// Whether we've detached from the chat messages alias
    detached: bool,
    /// DAG operations implementation
    dagops: D,
    /// Chat writer for function calls (lazily initialized)
    chat_writer: Option<FunCallsToChat<AWriter>>,
    /// Tools writer for function calls (lazily initialized)
    tools_writer: Option<FunCallsToTools>,
}

impl<D: DagOpsTrait> FunCallsBuilder<D> {
    /// Creates a new function call state manager
    ///
    /// # Arguments
    /// * `dagops` - DAG operations implementation
    ///
    /// # Returns
    /// A new `FunCallsBuilder` instance ready to process streaming function calls
    #[must_use]
    pub fn new(dagops: D) -> Self {
        Self {
            last_index: None,
            current_id: None,
            current_name: None,
            pending_arguments: None,
            new_item_called: false,
            detached: false,
            dagops,
            chat_writer: None,
            tools_writer: None,
        }
    }

    // =========================================================================
    // Private Helper Methods
    // =========================================================================

    /// Creates chat and tools writers lazily when needed
    ///
    /// This method initializes the writers only when they are first needed,
    /// which happens when we enter the detached state for function call processing.
    fn create_writers(&mut self) {
        if self.chat_writer.is_none() {
            let chat_awriter = AWriter::new_from_fd(777).expect("Failed to create writer from fd 777");
            self.chat_writer = Some(FunCallsToChat::new(chat_awriter));
        }
        if self.tools_writer.is_none() {
            self.tools_writer = Some(FunCallsToTools::new());
        }
    }


    /// Attempts to call `new_item` if both ID and name are available
    ///
    /// This method handles the coordination between ID and name arrival,
    /// calling the writers' `new_item` methods when both are present.
    ///
    /// # Errors
    /// Returns an error if the writers' `new_item` or `arguments_chunk` methods fail
    fn try_call_new_item(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.new_item_called {
            // Handle DAG detachment before calling writers
            if !self.detached {
                self.create_writers();
                self.setup_loop_iteration_in_dag()?;
                self.detached = true;
            }

            if let (Some(id), Some(name)) = (&self.current_id, &self.current_name) {
                if let (Some(ref mut chat_writer), Some(ref mut tools_writer)) = 
                    (&mut self.chat_writer, &mut self.tools_writer) {
                    chat_writer.new_item(id, name, &mut self.dagops)?;
                    tools_writer.new_item(id, name, &mut self.dagops)?;
                    self.new_item_called = true;

                    // Send any pending arguments that were accumulated before new_item
                    if let Some(ref args) = self.pending_arguments {
                        chat_writer.arguments_chunk(args)?;
                        tools_writer.arguments_chunk(args)?;
                        self.pending_arguments = None;
                    }

                    // Clear id and name - no longer needed after new_item
                    self.current_id = None;
                    self.current_name = None;
                }
            }
        }
        Ok(())
    }

    // =========================================================================
    // Public Interface Methods
    // =========================================================================

    /// Ends the current item if in direct mode
    ///
    /// In direct mode (when `index` has not been called), this method ends the item.
    /// In streaming mode (when `index` has been called), this method does nothing.
    ///
    /// # Errors
    /// Returns error if "`end_item_if_direct`" is called without "`new_item`" being called first
    pub fn end_item_if_direct(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.new_item_called {
            // Provide a more descriptive error message based on what's missing
            let missing_parts = match (&self.current_id, &self.current_name) {
                (None, None) => "both 'id' and 'name'",
                (None, Some(_)) => "'id'",
                (Some(_), None) => "'name'",
                (Some(_), Some(_)) => "unknown reason", // Should not happen
            };
            return Err(format!(
                "At the end of a 'tool_calls' item, {missing_parts} should be already given"
            )
            .into());
        }

        // Only end the item if we're in direct mode (not streaming mode)
        if !self.is_streaming_mode() {
            self.enforce_end_item()?;
        }

        Ok(())
    }

    /// Ends the current item regardless of mode
    ///
    /// This method always ends the item, whether in direct or streaming mode.
    /// It is needed for cases where the streaming processor needs to forcefully
    /// close items at the end of processing, even when in streaming mode.
    /// This ensures proper JSON structure completion in streaming scenarios.
    ///
    /// # Errors
    /// Returns error if "`enforce_end_item`" is called without "`new_item`" being called first
    pub fn enforce_end_item(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.new_item_called {
            return Err("enforce_end_item called without new_item being called first".into());
        }

        // Always end the item, regardless of mode
        if let (Some(ref mut chat_writer), Some(ref mut tools_writer)) = 
            (&mut self.chat_writer, &mut self.tools_writer) {
            chat_writer.end_item()?;
            tools_writer.end_item()?;
        }
        // Reset state for next function call
        self.reset_current_call_state();

        Ok(())
    }

    /// Sets the index and starts a new tool call if necessary
    ///
    /// # Arguments
    /// * `index` - The function call index
    ///
    /// # Errors
    /// Returns error if validation fails or writing operation fails
    pub fn index(&mut self, index: usize) -> Result<(), Box<dyn std::error::Error>> {
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
                        if let (Some(ref mut chat_writer), Some(ref mut tools_writer)) = 
                            (&mut self.chat_writer, &mut self.tools_writer) {
                            chat_writer.end_item()?;
                            tools_writer.end_item()?;
                        }
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
    /// # Arguments
    /// * `id` - The function call ID
    ///
    /// # Errors
    /// Returns error if validation fails
    pub fn id(&mut self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Check if ID is already set or new_item already called
        if self.new_item_called || self.current_id.is_some() {
            return Err("ID is already given".into());
        }

        // Store the ID
        self.current_id = Some(id.to_string());

        // ID is now stored only in current_call

        // Call new_item immediately if both id and name are now available
        self.try_call_new_item()?;

        Ok(())
    }

    /// Sets the name of the current function call
    ///
    /// # Arguments
    /// * `name` - The function call name
    ///
    /// # Errors
    /// Returns error if validation fails or writing operation fails
    pub fn name(&mut self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Check if name is already set or new_item already called
        if self.new_item_called || self.current_name.is_some() {
            return Err("Name is already given".into());
        }

        // Store the name
        self.current_name = Some(name.to_string());

        // Name is now stored only in current_call

        // Call new_item immediately if both id and name are now available
        self.try_call_new_item()?;

        Ok(())
    }

    /// Adds arguments to the current function call
    ///
    /// # Errors
    /// Returns error if writing operation fails
    pub fn arguments_chunk(&mut self, args: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        // Arguments are now stored only in current_call

        // Pass arguments directly to writers after new_item has been called
        if self.new_item_called {
            if let (Some(ref mut chat_writer), Some(ref mut tools_writer)) = 
                (&mut self.chat_writer, &mut self.tools_writer) {
                chat_writer.arguments_chunk(args)?;
                tools_writer.arguments_chunk(args)?;
            }
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
    pub fn end(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // "end" calls "end_item" if "end_item_if_direct" was not called
        if self.new_item_called {
            if let (Some(ref mut chat_writer), Some(ref mut tools_writer)) = 
                (&mut self.chat_writer, &mut self.tools_writer) {
                chat_writer.end_item()?;
                tools_writer.end_item()?;
            }
            self.new_item_called = false;
        }

        // Call end on writers (no dagops needed anymore)
        if let Some(ref mut chat_writer) = self.chat_writer {
            chat_writer.end()?;
        }
        if let Some(ref mut tools_writer) = self.tools_writer {
            tools_writer.end()?;
        }

        // Handle final DAG workflow processing
        self.end_workflow()?;

        Ok(())
    }

    // =========================================================================
    // Private Utility Methods
    // =========================================================================

    /// Checks if we are in streaming mode
    ///
    /// Streaming mode is enabled when the `index` method has been called at least once.
    ///
    /// # Returns
    /// `true` if streaming mode is enabled, `false` for direct mode
    fn is_streaming_mode(&self) -> bool {
        self.last_index.is_some()
    }

    /// Resets the state for the current function call
    fn reset_current_call_state(&mut self) {
        self.current_id = None;
        self.current_name = None;
        self.new_item_called = false;
        self.pending_arguments = None;
    }

    /// Handles final DAG workflow processing
    ///
    /// This method handles the final DAG workflow setup when all function calls are complete,
    /// including rerunning the model if any tool calls were processed.
    ///
    /// # Errors
    /// Returns error if DAG operations fail
    pub fn end_workflow(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Rerun model if we processed any tool calls (indicated by detached flag)
        if self.detached {
            let rerun_handle = self.dagops.instantiate_with_deps(
                ".gpt",
                std::collections::HashMap::from([
                    (".chat_messages.media".to_string(), 0),
                    (".chat_messages.toolspecs".to_string(), 0),
                ])
                .into_iter(),
            )?;
            self.dagops.alias(".output_messages", rerun_handle)?;
        }

        Ok(())
    }

    // =========================================================================
    // Utility Methods for DAG Operations
    // =========================================================================

    /// Sets up the DAG for a new iteration by detaching from previous workflows
    ///
    /// This method ensures that new function calls don't interfere with
    /// previous model workflows by detaching from the chat messages alias.
    /// This prevents confusing dependency relationships in the DAG.
    ///
    /// # Arguments
    /// * `dagops` - Mutable reference to the DAG operations implementation
    ///
    /// # Errors
    /// Returns error if the detach operation fails
    fn setup_loop_iteration_in_dag(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Detach from previous chat messages to avoid dependency confusion
        // This prevents the old "user prompt to messages" workflow from
        // appearing to depend on new chat messages we're about to create
        self.dagops.detach_from_alias(".chat_messages")?;
        Ok(())
    }
}
