//! Error types for markdown conversion action handlers.
//!
//! This module defines the error types that can occur during markdown conversion action handling,
//! including detailed context information for debugging.

use scan_json::rjiter::jiter::Peek;
use std::fmt;

/// Errors that can occur during markdown conversion action handling
#[derive(Debug)]
pub enum ActionError {
    /// Peek error for text
    PeekText(String),

    /// Expected string for text value but got something else
    TextNotString {
        got: Peek,
        index: usize,
        line: usize,
        column: usize,
    },

    /// Failed to start paragraph
    StartParagraph(String),

    /// Failed to write text
    WriteText(String),
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PeekText(e) => write!(f, "Peek error for text: {e}"),
            Self::TextNotString { got, index, line, column } => write!(
                f,
                "Expected string for 'text' value, got {got:?}, at index {index}, line {line}, column {column}"
            ),
            Self::StartParagraph(e) => write!(f, "Failed to start paragraph: {e}"),
            Self::WriteText(e) => write!(f, "Failed to write text: {e}"),
        }
    }
}

impl std::error::Error for ActionError {}
