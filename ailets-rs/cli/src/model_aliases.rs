//! Model alias resolution for the CLI.
//!
//! Each alias maps to a full model ID and the provider URL. A full model ID
//! that is not in the alias table is passed through verbatim; the caller is
//! responsible for supplying a URL in that case.

pub struct ResolvedAlias {
    /// Full model ID, or `None` for the `local` alias (user manages the model).
    pub model: Option<&'static str>,
    pub url: &'static str,
}

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/chat/completions";
const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
const GOOGLE_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";
const LOCAL_URL: &str = "http://localhost:11434/v1/chat/completions";

/// Resolve a short alias to a model ID and provider URL.
///
/// Returns `None` if `alias` is not a known alias (treat it as a full model ID).
#[must_use]
pub fn resolve_alias(alias: &str) -> Option<ResolvedAlias> {
    match alias {
        "gpt" => Some(ResolvedAlias {
            model: Some("gpt-5.4"),
            url: OPENAI_URL,
        }),
        "gpt-mini" => Some(ResolvedAlias {
            model: Some("gpt-5.4-mini"),
            url: OPENAI_URL,
        }),
        "fable" => Some(ResolvedAlias {
            model: Some("claude-fable-5"),
            url: ANTHROPIC_URL,
        }),
        "opus" => Some(ResolvedAlias {
            model: Some("claude-opus-4-8"),
            url: ANTHROPIC_URL,
        }),
        "sonnet" => Some(ResolvedAlias {
            model: Some("claude-sonnet-4-6"),
            url: ANTHROPIC_URL,
        }),
        "haiku" => Some(ResolvedAlias {
            model: Some("claude-haiku-4-5"),
            url: ANTHROPIC_URL,
        }),
        "gemini" => Some(ResolvedAlias {
            model: Some("gemini-2.5-flash"),
            url: GOOGLE_URL,
        }),
        "flash" => Some(ResolvedAlias {
            model: Some("gemini-3.5-flash"),
            url: GOOGLE_URL,
        }),
        "local" => Some(ResolvedAlias {
            model: None,
            url: LOCAL_URL,
        }),
        _ => None,
    }
}
