pub trait FunCallsWrite {
    /// Start a new function call item
    ///
    /// # Arguments
    /// * `id` - The unique identifier for the function call
    /// * `name` - The name of the function to be called
    ///
    /// # Errors
    /// Returns error if the writing operation fails
    fn new_item(&mut self, id: &str, name: &str) -> Result<(), Box<dyn std::error::Error>>;

    /// Add a chunk of arguments to the current function call
    ///
    /// # Arguments
    /// * `ac` - The arguments chunk to add
    ///
    /// # Errors
    /// Returns error if the writing operation fails
    fn arguments_chunk(&mut self, ac: &str) -> Result<(), Box<dyn std::error::Error>>;

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
}

impl<W: std::io::Write> FunCallsToChat<W> {
    /// Creates a new `FunCallsToChat` instance with the given writer
    #[must_use]
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: std::io::Write> FunCallsWrite for FunCallsToChat<W> {
    fn new_item(&mut self, id: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Start writing the function call JSON structure
        write!(
            self.writer,
            r#"[{{"type":"function","id":"{}","name":"{}"}},{{"arguments":""#,
            id, name
        )?;
        Ok(())
    }

    fn arguments_chunk(&mut self, ac: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Write arguments chunk directly to output
        write!(self.writer, "{}", ac)?;
        Ok(())
    }

    fn end_item(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Close the JSON structure and write newline
        writeln!(self.writer, "\"}}]")?;
        Ok(())
    }

    fn end(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // FunCallsToChat doesn't need to do anything special on end
        Ok(())
    }
}

/// `FunCallsGpt` forwards function call events to both `FunCallsToChat` and `DagOpsWrite`
pub struct FunCallsGpt<'a, W: std::io::Write, T: crate::dagops::DagOpsTrait> {
    chat_writer: FunCallsToChat<W>,
    dag_writer: crate::dagops::DagOpsWrite<'a, T>,
}

impl<'a, W: std::io::Write, T: crate::dagops::DagOpsTrait> FunCallsGpt<'a, W, T> {
    /// Create a new `FunCallsGpt` instance
    pub fn new(writer: W, dagops: &'a mut T) -> Self {
        Self {
            chat_writer: FunCallsToChat::new(writer),
            dag_writer: crate::dagops::DagOpsWrite::new(dagops),
        }
    }
}

impl<'a, W: std::io::Write, T: crate::dagops::DagOpsTrait> FunCallsWrite for FunCallsGpt<'a, W, T> {
    fn new_item(&mut self, id: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.chat_writer.new_item(id, name)?;
        self.dag_writer.new_item(id, name)?;
        Ok(())
    }

    fn arguments_chunk(&mut self, args: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.chat_writer.arguments_chunk(args)?;
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
