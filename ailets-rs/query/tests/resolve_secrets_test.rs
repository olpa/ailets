use query::resolve_secrets;

#[test]
fn errors_when_no_secret_available() {
    let get_env = |_k: &str| None;
    let result = resolve_secrets(
        "Bearer {{secret}}",
        "https://api.openai.com/v1/chat/completions",
        &get_env,
    );
    let err = result.unwrap_err();
    assert!(
        err.contains("OPENAI_API_KEY") && err.contains("LLM_API_KEY"),
        "error should name both candidates, got: {err}"
    );
}

#[test]
fn falls_back_to_llm_api_key() {
    let get_env = |k: &str| match k {
        "LLM_API_KEY" => Some("llm-fallback".to_string()),
        _ => None,
    };
    let result = resolve_secrets(
        "Bearer {{secret}}",
        "https://api.openai.com/v1/chat/completions",
        &get_env,
    );
    assert_eq!(result, Ok("Bearer llm-fallback".to_string()));
}

#[test]
fn uses_provider_specific_env_var() {
    let get_env = |k: &str| match k {
        "OPENAI_API_KEY" => Some("sk-test".to_string()),
        _ => None,
    };
    let result = resolve_secrets(
        "Bearer {{secret}}",
        "https://api.openai.com/v1/chat/completions",
        &get_env,
    );
    assert_eq!(result, Ok("Bearer sk-test".to_string()));
}
