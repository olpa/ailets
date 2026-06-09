use actor_io::{AReader, AWriter};
use actor_runtime::{ActorRuntime, StdHandle};
use embedded_io::{Read, Write};
use ureq::RequestExt;

/// Extract the provider name from a URL's domain.
/// `https://api.openai.com/v1/...` → `"openai"` (second-to-last label).
/// A bare host like `localhost:8000` → `"localhost"`.
pub fn provider_from_url(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split('/').next().unwrap_or(rest);
    let host = host.split(':').next().unwrap_or(host);
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() >= 2 {
        labels[labels.len() - 2].to_string()
    } else {
        labels.first().copied().unwrap_or("").to_string()
    }
}

/// Replace every `{{secret}}` in `value` with the resolved API key.
///
/// The key name is derived from the second-to-last domain label of `url`
/// (e.g. `api.openai.com` → `OPENAI_API_KEY`), with `LLM_API_KEY` as fallback.
/// `get_env` is injected for testability.
///
/// # Errors
/// Returns an error when `{{secret}}` appears but neither env var is set.
pub fn resolve_secrets(
    value: &str,
    url: &str,
    get_env: &dyn Fn(&str) -> Option<String>,
) -> Result<String, String> {
    if !value.contains("{{secret}}") {
        return Ok(value.to_string());
    }
    let provider = provider_from_url(url);
    let envvar1 = format!("{}_API_KEY", provider.to_uppercase());
    let secret = get_env(&envvar1)
        .or_else(|| get_env("LLM_API_KEY"))
        .ok_or_else(|| format!("Secret not found: {envvar1} or LLM_API_KEY"))?;
    Ok(value.replace("{{secret}}", &secret))
}

fn perform_request(spec: &serde_json::Value, writer: &mut AWriter) -> Result<(), String> {
    let url = spec["url"].as_str().ok_or("Missing or invalid 'url' field")?;
    let method = spec["method"].as_str().unwrap_or("POST");
    let headers = spec["headers"].as_object().ok_or("Missing 'headers' field")?;
    let body_obj = spec["body"]
        .as_object()
        .ok_or("Missing 'body' field (body_key streaming not supported in this version)")?;
    let body_bytes =
        serde_json::to_vec(body_obj).map_err(|e| format!("Failed to serialize body: {e}"))?;

    let get_env = |k: &str| std::env::var(k).ok();

    let method_parsed = ureq::http::Method::from_bytes(method.as_bytes())
        .map_err(|e| format!("Invalid HTTP method '{method}': {e}"))?;
    let mut builder = ureq::http::Request::builder()
        .method(method_parsed)
        .uri(url);
    for (key, val) in headers {
        if let Some(v) = val.as_str() {
            let resolved = resolve_secrets(v, url, &get_env)?;
            builder = builder.header(key.as_str(), resolved.as_str());
        }
    }
    let request = builder
        .body(body_bytes)
        .map_err(|e| format!("Failed to build HTTP request: {e}"))?;

    let response = request
        .with_default_agent()
        .configure()
        .http_status_as_error(false)
        .run()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = response.status().as_u16();
    let body = response.into_body();

    if !(200..300).contains(&status) {
        let mut buf = vec![0u8; 1024];
        let n = {
            use std::io::Read as _;
            body.into_reader().read(&mut buf).unwrap_or(0)
        };
        buf.truncate(n);
        let text = String::from_utf8_lossy(&buf);
        return Err(format!("HTTP {status} {text}"));
    }

    let mut reader = body.into_reader();
    let mut buf = [0u8; 8192];
    loop {
        use std::io::Read as _;
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => writer
                .write_all(&buf[..n])
                .map_err(|e| format!("Failed to write response chunk: {e:?}"))?,
            Err(e) => return Err(format!("Failed to read response body: {e}")),
        }
    }
    Ok(())
}

/// Query actor entry point: reads a JSON spec from stdin, performs the HTTP
/// request, and streams the response body chunk-by-chunk to stdout.
///
/// # Errors
/// Returns an error on I/O failure, invalid spec, secret resolution failure,
/// or non-2xx HTTP status.
pub fn execute(runtime: &dyn ActorRuntime) -> Result<(), String> {
    let mut reader = AReader::new_from_std(runtime, StdHandle::Stdin);
    let mut writer = AWriter::new_from_std(runtime, StdHandle::Stdout);

    let mut input = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => input.extend_from_slice(&buf[..n]),
            Err(e) => return Err(format!("Failed to read query spec: {e:?}")),
        }
    }

    let spec: serde_json::Value =
        serde_json::from_slice(&input).map_err(|e| format!("Failed to parse query spec: {e}"))?;

    perform_request(&spec, &mut writer)
}
