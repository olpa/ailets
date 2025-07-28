//! Composite function call writer
//!
//! This module provides a composite writer that forwards function calls to multiple
//! destinations, enabling function calls to be processed simultaneously for both
//! chat display and internal DAG tracking.

use crate::dagops::DagOpsTrait;
use crate::fcw_chat::FunCallsToChat;
use crate::fcw_dag::FunCallsToDag;
use crate::fcw_trait::{FunCallResult, FunCallsWrite};
use std::io::Write;

/// Composite writer that forwards function calls to multiple destinations
///
/// This writer combines chat-style JSON output with DAG operations, enabling
/// function calls to be processed simultaneously for:
/// - Chat display (JSON format)
/// - Internal DAG tracking and workflow management
///
/// The composite pattern ensures both writers receive the same data in the
/// correct order, with fail-fast error handling.
///
/// # Type Parameters
/// * `W` - Writer type for chat output (must implement `std::io::Write`)
/// * `T` - DAG operations trait implementation
pub struct FunCallsFork<'a, W: Write, T: DagOpsTrait> {
    chat_writer: FunCallsToChat<W>,
    dag_writer: FunCallsToDag<'a, T>,
}

impl<'a, W: Write, T: DagOpsTrait> FunCallsFork<'a, W, T> {
    /// Create a new composite function call writer
    ///
    /// # Arguments
    /// * `writer` - Output writer for chat-style JSON formatting
    /// * `dagops` - Mutable reference to DAG operations handler
    ///
    /// # Returns
    /// A composite writer that will forward all operations to both the chat
    /// writer and the DAG writer simultaneously
    pub fn new(writer: W, dagops: &'a mut T) -> Self {
        Self {
            chat_writer: FunCallsToChat::new(writer),
            dag_writer: FunCallsToDag::new(dagops),
        }
    }
}

impl<'a, W: Write, T: DagOpsTrait> FunCallsWrite for FunCallsFork<'a, W, T> {
    fn new_item(&mut self, id: &str, name: &str) -> FunCallResult {
        // Forward to both writers with fail-fast error handling
        self.chat_writer.new_item(id, name)?;
        self.dag_writer.new_item(id, name)?;
        Ok(())
    }

    fn arguments_chunk(&mut self, chunk: &str) -> FunCallResult {
        // Forward argument chunks to both writers
        self.chat_writer.arguments_chunk(chunk)?;
        self.dag_writer.arguments_chunk(chunk)?;
        Ok(())
    }

    fn end_item(&mut self) -> FunCallResult {
        // Finalize current item in both writers
        self.chat_writer.end_item()?;
        self.dag_writer.end_item()?;
        Ok(())
    }

    fn end(&mut self) -> FunCallResult {
        // Complete processing in both writers
        self.chat_writer.end()?;
        self.dag_writer.end()?;
        Ok(())
    }
}
