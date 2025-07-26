use crate::funcalls_write::FunCallsWrite;

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct FunCalls {
    pub last_index: Option<usize>,
    current_id: Option<String>,
    current_name: Option<String>,
    pending_arguments: Option<String>,
    new_item_called: bool,
}

impl FunCalls {
    /// Creates a new empty collection of function calls
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

    /// Calls `new_item` immediately if both id and name are now available
    fn try_call_new_item(
        &mut self,
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.new_item_called {
            if let (Some(id), Some(name)) = (&self.current_id, &self.current_name) {
                writer.new_item(id, name)?;
                self.new_item_called = true;

                // Send any pending arguments that were accumulated before new_item
                if let Some(ref args) = self.pending_arguments {
                    writer.arguments_chunk(args)?;
                    self.pending_arguments = None;
                }

                // Clear id and name - no longer needed after new_item
                self.current_id = None;
                self.current_name = None;
            }
        }
        Ok(())
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
        self.current_id = None;
        self.current_name = None;
        self.new_item_called = false;
        self.pending_arguments = None;
        Ok(())
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
        if !self.new_item_called {
            return Err("end_item called without new_item being called first".into());
        }

        writer.end_item()?;
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
                    if self.new_item_called {
                        writer.end_item()?;
                    }
                    self.current_id = None;
                    self.current_name = None;
                    self.new_item_called = false;
                    self.pending_arguments = None;
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
        if self.new_item_called || self.current_id.is_some() {
            return Err("ID is already given".into());
        }

        // Store the ID
        self.current_id = Some(id.to_string());

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
        if self.new_item_called || self.current_name.is_some() {
            return Err("Name is already given".into());
        }

        // Store the name
        self.current_name = Some(name.to_string());

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
        if self.new_item_called {
            writer.arguments_chunk(args)?;
        } else {
            // Store arguments until new_item is called
            match &mut self.pending_arguments {
                Some(existing) => existing.push_str(args),
                None => self.pending_arguments = Some(args.to_string()),
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
        writer: &mut dyn FunCallsWrite,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // "end" calls "end_item" if "end_item" was not called
        if self.new_item_called {
            writer.end_item()?;
            self.new_item_called = false;
        }
        Ok(())
    }
}
