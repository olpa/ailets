//! Collect function calls from an AI model response
//!
//! - Tracking individual function calls with their IDs, names, and arguments
//! - Managing collections of function calls
//! - Incrementally building function calls through delta updates
//! - Writing function call data directly to output writers
//!
//! The primary structures are:
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
    /// * `id` - The unique identifier for the function call
    /// * `name` - The name of the function to be called
    ///
    /// # Errors
    /// Returns error if the writing operation fails
    fn new_item(&mut self, id: String, name: String) -> Result<(), Box<dyn std::error::Error>>;

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

    /// Finalize all function call processing
    ///
    /// # Errors
    /// Returns error if the writing operation fails
    fn end(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}

/// Implementation of `FunCallsWrite` that writes to a chat-style format
///
/// This implementation writes function calls in the format expected by chat systems,
/// with function call data written as JSON lines.
pub struct FunCallsToChat<W: std::io::Write> {
    writer: W,
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
            current_id: None,
            current_name: None,
            current_arguments: String::new(),
        }
    }
}

impl<W: std::io::Write> FunCallsWrite for FunCallsToChat<W> {
    fn new_item(&mut self, id: String, name: String) -> Result<(), Box<dyn std::error::Error>> {
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

    fn end(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // FunCallsToChat doesn't need to do anything special on end
        Ok(())
    }
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct FunCalls {
    last_index: Option<usize>,
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
                writer.new_item(id.clone(), name.clone())?;
                self.new_item_called = true;

                // Send any pending arguments that were accumulated before new_item
                if let Some(ref args) = self.pending_arguments {
                    writer.arguments_chunk(args.clone())?;
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
            writer.arguments_chunk(args.to_string())?;
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

/// `FunCallsGpt` forwards function call events to both `FunCallsToChat` and `DagOpsWrite`
pub struct FunCallsGpt<'a, W: std::io::Write, T: crate::dagops::DagOpsTrait> {
    chat_writer: crate::funcalls::FunCallsToChat<W>,
    dag_writer: crate::dagops::DagOpsWrite<'a, T>,
}

impl<'a, W: std::io::Write, T: crate::dagops::DagOpsTrait> FunCallsGpt<'a, W, T> {
    /// Create a new `FunCallsGpt` instance
    pub fn new(writer: W, dagops: &'a mut T) -> Self {
        Self {
            chat_writer: crate::funcalls::FunCallsToChat::new(writer),
            dag_writer: crate::dagops::DagOpsWrite::new(dagops),
        }
    }
}

impl<'a, W: std::io::Write, T: crate::dagops::DagOpsTrait> FunCallsWrite for FunCallsGpt<'a, W, T> {
    fn new_item(&mut self, id: String, name: String) -> Result<(), Box<dyn std::error::Error>> {
        self.chat_writer.new_item(id.clone(), name.clone())?;
        self.dag_writer.new_item(id, name)?;
        Ok(())
    }

    fn arguments_chunk(&mut self, args: String) -> Result<(), Box<dyn std::error::Error>> {
        self.chat_writer.arguments_chunk(args.clone())?;
        self.dag_writer.arguments_chunk(args)?;
        Ok(())
    }

    fn end_item(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.chat_writer.end_item()?;
        self.dag_writer.end_item()?;
        Ok(())
    }

    fn end(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.chat_writer.end()?;
        self.dag_writer.end()?;
        Ok(())
    }
}
