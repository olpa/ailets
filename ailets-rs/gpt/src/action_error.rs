//! Error types for GPT stream action handlers.
//!
//! This module defines the error types that can occur during GPT stream action handling,
//! including detailed context information for debugging.

use scan_json::rjiter::jiter::Peek;
use std::fmt;

/// Errors that can occur during GPT stream action handling
#[derive(Debug)]
pub enum ActionError {
    /// Failed to begin a message
    BeginMessage(String),

    /// Failed to end a message
    EndMessage(String),

    /// Error getting role value
    RoleValue(String),

    /// Failed to set role
    SetRole(String),

    /// Peek error for content
    PeekContent(String),

    /// Error consuming null value
    ConsumeNull(String),

    /// Expected string for content value but got something else
    ContentNotString {
        got: Peek,
        index: usize,
        line: usize,
        column: usize,
    },

    /// Failed to begin text chunk
    BeginTextChunk(String),

    /// Failed to write content bytes
    WriteContentBytes(String),

    /// Expected string as function id
    FunctionIdNotString(String),

    /// Error handling function id
    HandleFunctionId(String),

    /// Expected string as function name
    FunctionNameNotString(String),

    /// Error handling function name
    HandleFunctionName(String),

    /// Peek error for arguments
    PeekArguments(String),

    /// Expected string for arguments value but got something else
    ArgumentsNotString {
        got: Peek,
        index: usize,
        line: usize,
        column: usize,
    },

    /// Failed to write arguments bytes
    WriteArgumentsBytes(String),

    /// Expected integer as function index
    FunctionIndexNotInt(String),

    /// Function index too large for usize
    FunctionIndexTooLarge,

    /// Can't convert function index to usize
    FunctionIndexConversion(String),

    /// Error handling function call index
    HandleFunctionIndex(String),

    /// Failed to end tool call
    EndToolCall(String),
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BeginMessage(e) => write!(f, "Failed to begin message: {e}"),
            Self::EndMessage(e) => write!(f, "Failed to end message: {e}"),
            Self::RoleValue(e) => write!(f, "Error getting role value. Expected string, got: {e}"),
            Self::SetRole(e) => write!(f, "Failed to set role: {e}"),
            Self::PeekContent(e) => write!(f, "Peek error for content: {e}"),
            Self::ConsumeNull(e) => write!(f, "Error consuming null: {e}"),
            Self::ContentNotString { got, index, line, column } => write!(
                f,
                "Expected string for 'content' value, got {got:?}, at index {index}, line {line}, column {column}"
            ),
            Self::BeginTextChunk(e) => write!(f, "Failed to begin text chunk: {e}"),
            Self::WriteContentBytes(e) => write!(f, "Failed to write content bytes: {e}"),
            Self::FunctionIdNotString(e) => write!(f, "Expected string as the function id, got {e}"),
            Self::HandleFunctionId(e) => write!(f, "Error handling function id: {e}"),
            Self::FunctionNameNotString(e) => write!(f, "Expected string as the function name, got {e}"),
            Self::HandleFunctionName(e) => write!(f, "Error handling function name: {e}"),
            Self::PeekArguments(e) => write!(f, "Peek error for arguments: {e}"),
            Self::ArgumentsNotString { got, index, line, column } => write!(
                f,
                "Expected string for 'arguments' value, got {got:?}, at index {index}, line {line}, column {column}"
            ),
            Self::WriteArgumentsBytes(e) => write!(f, "Failed to write arguments bytes: {e}"),
            Self::FunctionIndexNotInt(e) => write!(f, "Expected integer as function index, got {e}"),
            Self::FunctionIndexTooLarge => write!(f, "Function index too large for usize"),
            Self::FunctionIndexConversion(e) => write!(f, "Can't convert function index to usize: {e}"),
            Self::HandleFunctionIndex(e) => write!(f, "Error handling function call index: {e}"),
            Self::EndToolCall(e) => write!(f, "Failed to end tool call: {e}"),
        }
    }
}

impl std::error::Error for ActionError {}
