//! Error types for messages to query conversion action handlers.
//!
//! This module defines the error types that can occur during messages to query conversion action handling,
//! including detailed context information for debugging.

use scan_json::rjiter::jiter::Peek;
use std::fmt;

/// Errors that can occur during messages to query conversion action handling
#[derive(Debug)]
pub enum ActionError {
    /// Error getting role value
    GetRole(String),

    /// Failed to handle role
    HandleRole(String),

    /// Failed to begin item
    BeginItem(String),

    /// Failed to end item
    EndItem(String),

    /// Error getting type value
    GetType(String),

    /// Failed to add item type
    AddItemType(String),

    /// Peek error for text
    PeekText(String),

    /// Expected string for text value but got something else
    TextNotString {
        got: Peek,
        index: usize,
        line: usize,
        column: usize,
    },

    /// Failed to begin text
    BeginText(String),

    /// Failed to write text bytes
    WriteTextBytes(String),

    /// Failed to end text
    EndText(String),

    /// Peek error for image_url
    PeekImageUrl(String),

    /// Expected string for image_url value but got something else
    ImageUrlNotString {
        got: Peek,
        index: usize,
        line: usize,
        column: usize,
    },

    /// Failed to begin image_url
    BeginImageUrl(String),

    /// Failed to write image_url bytes
    WriteImageUrlBytes(String),

    /// Failed to end image_url
    EndImageUrl(String),

    /// Error getting key value
    GetKey(String),

    /// Failed to set image key
    SetImageKey(String),

    /// Error getting content_type
    GetContentType(String),

    /// Failed to add content_type
    AddContentType(String),

    /// Error getting detail
    GetDetail(String),

    /// Failed to add detail
    AddDetail(String),

    /// Error getting function id
    GetFunctionId(String),

    /// Failed to add function id
    AddFunctionId(String),

    /// Error getting function name
    GetFunctionName(String),

    /// Failed to add function name
    AddFunctionName(String),

    /// Peek error for arguments
    PeekArguments(String),

    /// Expected string for arguments value but got something else
    ArgumentsNotString {
        got: Peek,
        index: usize,
        line: usize,
        column: usize,
    },

    /// Failed to begin function arguments
    BeginFunctionArguments(String),

    /// Failed to write arguments bytes
    WriteArgumentsBytes(String),

    /// Failed to end function arguments
    EndFunctionArguments(String),

    /// Error getting toolspec key
    GetToolspecKey(String),

    /// Failed to set toolspec key
    SetToolspecKey(String),

    /// Error getting tool_call_id
    GetToolCallId(String),

    /// Failed to add tool_call_id
    AddToolCallId(String),
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GetRole(e) => write!(f, "Error getting role value: {e}"),
            Self::HandleRole(e) => write!(f, "Failed to handle role: {e}"),
            Self::BeginItem(e) => write!(f, "Failed to begin item: {e}"),
            Self::EndItem(e) => write!(f, "Failed to end item: {e}"),
            Self::GetType(e) => write!(f, "Error getting type value: {e}"),
            Self::AddItemType(e) => write!(f, "Failed to add item type: {e}"),
            Self::PeekText(e) => write!(f, "Peek error for text: {e}"),
            Self::TextNotString { got, index, line, column } => write!(
                f,
                "Expected string for 'text' value, got {got:?}, at index {index}, line {line}, column {column}"
            ),
            Self::BeginText(e) => write!(f, "Failed to begin text: {e}"),
            Self::WriteTextBytes(e) => write!(f, "Failed to write text bytes: {e}"),
            Self::EndText(e) => write!(f, "Failed to end text: {e}"),
            Self::PeekImageUrl(e) => write!(f, "Peek error for image_url: {e}"),
            Self::ImageUrlNotString { got, index, line, column } => write!(
                f,
                "Expected string for 'image_url' value, got {got:?}, at index {index}, line {line}, column {column}"
            ),
            Self::BeginImageUrl(e) => write!(f, "Failed to begin image_url: {e}"),
            Self::WriteImageUrlBytes(e) => write!(f, "Failed to write image_url bytes: {e}"),
            Self::EndImageUrl(e) => write!(f, "Failed to end image_url: {e}"),
            Self::GetKey(e) => write!(f, "Error getting key value: {e}"),
            Self::SetImageKey(e) => write!(f, "Failed to set image key: {e}"),
            Self::GetContentType(e) => write!(f, "Error getting content_type: {e}"),
            Self::AddContentType(e) => write!(f, "Failed to add content_type: {e}"),
            Self::GetDetail(e) => write!(f, "Error getting detail: {e}"),
            Self::AddDetail(e) => write!(f, "Failed to add detail: {e}"),
            Self::GetFunctionId(e) => write!(f, "Error getting function id: {e}"),
            Self::AddFunctionId(e) => write!(f, "Failed to add function id: {e}"),
            Self::GetFunctionName(e) => write!(f, "Error getting function name: {e}"),
            Self::AddFunctionName(e) => write!(f, "Failed to add function name: {e}"),
            Self::PeekArguments(e) => write!(f, "Peek error for arguments: {e}"),
            Self::ArgumentsNotString { got, index, line, column } => write!(
                f,
                "Expected string for 'arguments' value, got {got:?}, at index {index}, line {line}, column {column}"
            ),
            Self::BeginFunctionArguments(e) => write!(f, "Failed to begin function arguments: {e}"),
            Self::WriteArgumentsBytes(e) => write!(f, "Failed to write arguments bytes: {e}"),
            Self::EndFunctionArguments(e) => write!(f, "Failed to end function arguments: {e}"),
            Self::GetToolspecKey(e) => write!(f, "Error getting toolspec key: {e}"),
            Self::SetToolspecKey(e) => write!(f, "Failed to set toolspec key: {e}"),
            Self::GetToolCallId(e) => write!(f, "Error getting tool_call_id: {e}"),
            Self::AddToolCallId(e) => write!(f, "Failed to add tool_call_id: {e}"),
        }
    }
}

impl std::error::Error for ActionError {}
