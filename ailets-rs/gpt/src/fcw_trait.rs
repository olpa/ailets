//! Function call writing trait definition

pub type FunCallResult = Result<(), String>;

pub trait FunCallsWrite {
    /// Initialize a new function call with the given ID and name
    ///
    /// # Arguments
    /// * `id` - Unique identifier for the function call (will be JSON-escaped)
    /// * `name` - Name of the function to be called (will be JSON-escaped)
    /// * `dagops` - Mutable reference to DAG operations implementation
    ///
    /// # Errors
    /// Returns an error if the underlying writer fails
    fn new_item<T: crate::dagops::DagOpsTrait>(
        &mut self,
        id: &str,
        name: &str,
        dagops: &mut T,
    ) -> FunCallResult;

    /// Add a chunk of arguments to the current function call
    ///
    /// This method can be called multiple times to stream large arguments.
    /// All chunks will be concatenated and JSON-escaped in the final output.
    ///
    /// # Arguments
    /// * `chunk` - The arguments chunk to append (will be JSON-escaped)
    ///
    /// # Errors
    /// Returns an error if the underlying writer fails
    fn arguments_chunk(&mut self, chunk: &[u8]) -> FunCallResult;

    /// Finalize the current function call item
    ///
    /// # Errors
    /// Returns an error if the underlying writer fails
    fn end_item(&mut self) -> FunCallResult;

    /// Complete all function call processing
    ///
    /// This is called once at the end of all function call processing.
    ///
    /// # Errors
    /// Returns an error if the underlying writer fails
    fn end(&mut self) -> FunCallResult;
}
