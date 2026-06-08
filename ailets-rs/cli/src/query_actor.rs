//! Stub "query" actor: stands in for the not-yet-built HTTP-calling actor
//! that would send the query JSON (produced by `messages_to_query`) to an
//! LLM provider and stream back its response.
//!
//! This stub ignores its input and always emits the same canned
//! chat-completion SSE stream, shaped the way `gpt.response_to_messages`
//! expects, as if the provider replied to the prompt "hello!" with
//! "Hello! How can I help you today?".
//!
//! Replacing this with a real HTTP-calling actor (auth/secrets, streaming
//! SSE parsing, error handling, retries) is separate, larger follow-up work
//! with its own task.

use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use std::io::{Read as _, Write as _};

const CANNED_RESPONSE: &str = concat!(
    r#"data: {"id":"chatcmpl-stub","object":"chat.completion.chunk","created":0,"model":"stub","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}"#, "\n\n",
    r#"data: {"id":"chatcmpl-stub","object":"chat.completion.chunk","created":0,"model":"stub","choices":[{"index":0,"delta":{"content":"Hello! How can I help you today?"},"finish_reason":null}]}"#, "\n\n",
    r#"data: {"id":"chatcmpl-stub","object":"chat.completion.chunk","created":0,"model":"stub","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#, "\n\n",
    "data: [DONE]\n\n",
);

/// Stub "query" actor entry point.
///
/// Drains stdin (the query JSON `messages_to_query` produced) without
/// inspecting it, then writes a fixed canned LLM-response stream to stdout.
///
/// # Errors
/// Returns an error if reading stdin or writing stdout fails.
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let mut reader = AReader::new_from_std(runtime, StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(runtime, StdHandle::Stdout);

    let mut discard = Vec::new();
    reader
        .read_to_end(&mut discard)
        .map_err(|e| format!("Failed to read query input: {e}"))?;

    writer
        .write_all(CANNED_RESPONSE.as_bytes())
        .map_err(|e| format!("Failed to write canned response: {e}"))?;

    Ok(())
}
