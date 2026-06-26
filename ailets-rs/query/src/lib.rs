use actor_io::{AReader, AWriter};
use actor_runtime::{var_access::read_var, ActorRuntime, StdHandle};
use std::io::{Read, Write};
use ureq::RequestExt;

#[derive(serde::Deserialize)]
struct QuerySpec {
    url: String,
    method: String,
    headers: serde_json::Map<String, serde_json::Value>,
    body: Box<serde_json::value::RawValue>,
}

/// Extract the provider name from a URL's domain.
/// `https://api.openai.com/v1/...` → `"openai"` (second-to-last label).
/// A bare host like `localhost:8000` → `"localhost"`.
#[must_use]
pub fn provider_from_url(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split('/').next().unwrap_or(rest);
    let host = host.split(':').next().unwrap_or(host);
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() >= 2 {
        labels
            .get(labels.len() - 2)
            .copied()
            .unwrap_or("")
            .to_string()
    } else {
        labels.first().copied().unwrap_or("").to_string()
    }
}

/// Replace every `{{secret}}` in `value` with the resolved API key.
///
/// The key name is derived from the second-to-last domain label of `url`
/// (e.g. `api.openai.com` → `OPENAI_API_KEY`), with `LLM_API_KEY` as fallback.
/// `get_var` is injected for testability.
///
/// # Errors
/// Returns an error when `{{secret}}` appears but neither var is set.
pub fn resolve_secrets(
    value: &str,
    url: &str,
    get_var: &dyn Fn(&str) -> Option<String>,
) -> Result<String, String> {
    if !value.contains("{{secret}}") {
        return Ok(value.to_string());
    }
    let provider = provider_from_url(url);
    let envvar = format!("{}_API_KEY", provider.to_uppercase());
    let secret = get_var(&envvar)
        .or_else(|| get_var("LLM_API_KEY"))
        .ok_or_else(|| format!("Secret not found: {envvar} or LLM_API_KEY"))?;
    Ok(value.replace("{{secret}}", &secret))
}

fn perform_request(
    spec: &QuerySpec,
    writer: &mut impl Write,
    agent: &ureq::Agent,
    get_var: &dyn Fn(&str) -> Option<String>,
) -> Result<(), String> {
    let url = &spec.url;
    let method = &spec.method;
    let headers = &spec.headers;

    let method_parsed = ureq::http::Method::from_bytes(method.as_bytes())
        .map_err(|e| format!("Invalid HTTP method '{method}': {e}"))?;
    let mut builder = ureq::http::Request::builder()
        .method(method_parsed)
        .uri(url);
    for (key, val) in headers {
        if let Some(v) = val.as_str() {
            let resolved = resolve_secrets(v, url, get_var)?;
            builder = builder.header(key.as_str(), resolved.as_str());
        }
    }
    let request = builder
        .body(spec.body.get())
        .map_err(|e| format!("Failed to build HTTP request: {e}"))?;

    let response = request
        .with_agent(agent)
        .configure()
        .http_status_as_error(false)
        .run()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status().as_u16();
    let body = response.into_body();

    if !(200..300).contains(&status) {
        let mut buf = Vec::new();
        body.into_reader()
            .take(1024)
            .read_to_end(&mut buf)
            .unwrap_or(0);
        let text = String::from_utf8_lossy(&buf);
        return Err(format!("HTTP {status} {text}"));
    }

    std::io::copy(&mut body.into_reader(), writer)
        .map_err(|e| format!("Failed to stream response body: {e}"))?;
    Ok(())
}

/// Query actor entry point: reads a JSON spec from stdin, performs the HTTP
/// request, and streams the response body chunk-by-chunk to stdout.
///
/// # Errors
/// Returns an error on I/O failure, invalid spec, secret resolution failure,
/// or non-2xx HTTP status.
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let reader = AReader::new_from_std(runtime, StdHandle::Stdin);
    let writer = AWriter::new_from_std(runtime, StdHandle::Stdout);
    let get_var = |k: &str| read_var(runtime, k).ok().flatten();
    let result = execute_impl(reader, writer, &ureq::Agent::new_with_defaults(), &get_var);
    if let Err(ref e) = result {
        let mut log = AWriter::new_from_std(runtime, StdHandle::Log);
        if writeln!(log, "{e}").is_err() {}
    }
    result
}

/// Like [`execute`] but uses a caller-supplied agent, enabling transport injection for tests.
///
/// # Errors
/// Returns an error on I/O failure, invalid spec, secret resolution failure,
/// or non-2xx HTTP status.
pub fn execute_impl(
    reader: impl Read,
    mut writer: impl Write,
    agent: &ureq::Agent,
    get_var: &dyn Fn(&str) -> Option<String>,
) -> Result<(), String> {
    let spec: QuerySpec =
        serde_json::from_reader(reader).map_err(|e| format!("Failed to parse query spec: {e}"))?;

    perform_request(&spec, &mut writer, agent, get_var)
}
