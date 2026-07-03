# A185 JSON escaping/unescaping audit

Audit of `ailets-rs` for JSON string escaping/unescaping: existing implementations, TODO markers, and missing-escape bugs. Related to https://github.com/olpa/ailets/issues/185.

## Existing implementations

| File | Lines | Notes |
|---|---|---|
| `gpt/src/fcw_chat.rs` | 6-9 | Hand-rolled `escape_json_string`, used for `id`/`name` in function-call JSON. Incomplete — see below. |
| `gpt/src/fcw_tools.rs` | 8-11 | Duplicate of the same `escape_json_string`, used for tool-spec JSON. Same gap. |
| `gpt/src/fcw_tools.rs` | 97-116 | Unescapes a JSON string via `jiter::Jiter::known_str()` (wraps raw arg chunk in quotes first). Delegates correctly to `jiter`, including `\uXXXX`/surrogate pairs. |
| `messages_to_query/src/structure_builder.rs` | 244, 268, 284, 468, 482, 545, 572, 639, 669-672, 724 | Uses `serde_json::to_string(&value)` to produce escaped JSON string literals. Correct and complete. |
| `gpt/src/handlers.rs`, `messages_to_query/src/handlers.rs` | various `rjiter.write_long_bytes(writer)` call sites | Streams already-escaped JSON string bytes straight through from source to output — safe passthrough, not manual re-encoding. Verified by `messages_to_query/tests/actor_test.rs:180-192` and `messages_to_markdown/tests/actor_test.rs:104-115`. |
| `actor_runtime/src/lib.rs` (66-70), `actor_runtime/src/dagops.rs` (102-106) | — | Uses `serde_json::json!`/`Map`/`to_vec` — safe, delegated. |
| `cli/src/user_input.rs` | 98-99 | Static JSON literals with hardcoded content — no escaping needed. |

No hand-rolled JSON *parser* exists anywhere in the workspace; all parsing goes through `scan_json`/`rjiter`/`jiter` or `serde_json`, both of which correctly handle `\"`, `\\`, `\n`, `\t`, `\uXXXX`, and surrogate pairs.

## TODO markers

- `gpt/src/fcw_chat.rs:6` — `// TODO https://github.com/olpa/ailets/issues/185`, directly above the incomplete `escape_json_string`.
- `gpt/src/fcw_tools.rs:8` — same TODO, above the duplicate incomplete escaper.
- `gpt/src/fcw_tools.rs:100` — same TODO again, above the "wrap in quotes + Jiter unescape" workaround for `arguments_chunk`.
- `to_doc_item/src/lib.rs:14` — explicit doc comment: *"Note: raw bytes are embedded without JSON escaping. Proper escaping is deferred."* The clearest documented gap in the workspace.

## Bugs — missing escape

### `to_doc_item/src/lib.rs:31-37` — `write_attr`
```rust
fn write_attr(writer: &mut impl std::io::Write, key: &str, value: &str) -> Result<(), String> {
    wr(writer, b",\"")?;
    wr(writer, key.as_bytes())?;
    wr(writer, b"\":\"")?;
    wr(writer, value.as_bytes())?;   // raw, unescaped
    wr(writer, b"\"")
}
```
`key`/`value` come from `AILETS_DOC_ITEM_*` runtime vars and are written verbatim inside JSON quotes. A `"`, `\`, or control character in the var value corrupts the emitted JSON.

### `to_doc_item/src/lib.rs:92` — content copy
```rust
std::io::copy(&mut reader, &mut writer).map_err(...)?;
wr(&mut writer, br#""}]"#)
```
Raw content bytes read from the input pipe are copied byte-for-byte into the `"text":"<here>"` JSON string value with **no escaping at all**. Most severe finding in the audit — any quote/backslash/newline/control char in the source content breaks the JSON.

### `gpt/src/structure_builder.rs:137-141` — `begin_text_chunk` role header
```rust
self.stdout.write_all(b"[{\"type\":\"ctl\"},{\"role\":\"")?;
self.stdout.write_all(role.as_bytes())?;   // raw, unescaped
self.stdout.write_all(b"\"}]\n")?;
```
`role` comes from the (LLM-controlled) response JSON via `rjiter.next_str()`. If it ever contains `"` or `\`, output JSON is corrupted.

### `messages_to_query/src/structure_builder.rs:320` — `begin_message`
```rust
let role_str = format!(r#"{{"role":"{role}""#);
```
`role` is read from user input JSON and interpolated raw — inconsistent with the rest of the file, which uses `serde_json::to_string` for values. Genuine injection risk since `role` is user-controlled input.

### `messages_to_query/src/structure_builder.rs:245, 285` — header/param names
```rust
let header_part = format!(r#", "{header_name}": {value_json}"#);   // line 245
let param_part = format!(r#", "{param_name}": {value_json}"#);     // line 285
```
Values are escaped via `serde_json::to_string`, but the *keys* (`header_name`/`param_name`, derived from `AILETS_HTTP_HEADER_*`/`AILETS_LLM_*` var names) are interpolated raw. Lower risk (server-configured var names, not request content) but still a latent injection point.

### `messages_to_query/src/structure_builder.rs:479, 569` — `key_part`
```rust
let key_part = format!(r#","{key}":"#);
```
`key` in `begin_item_logic` and `add_item_attribute` is written raw as a JSON key. All current call sites (`messages_to_query/src/handlers.rs:67,203,224,245,266,368`) pass only hardcoded literal keys, so not exploitable today — but the `key: String` signature offers no protection against a future externally-derived key.

## Bugs — incomplete escaping

### `gpt/src/fcw_chat.rs:7-9` and `gpt/src/fcw_tools.rs:9-11` — `escape_json_string`
```rust
fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
```
Only escapes `\` and `"`. Missing:
- Control characters `\n`, `\r`, `\t`, `\b`, `\f` and any other byte `< 0x20` (JSON spec requires these escaped, e.g. via `\u00XX`).

Tests (`gpt/tests/fcw_chat_test.rs:151-171`, `:174-193`) only cover quotes and backslashes — the control-character gap is untested as well as unimplemented.

## Summary table

| File | Line(s) | Category |
|---|---|---|
| `to_doc_item/src/lib.rs` | 14, 31-37 | missing escape — attr values raw into JSON |
| `to_doc_item/src/lib.rs` | 92 | missing escape — content bytes raw into JSON string |
| `gpt/src/structure_builder.rs` | 137-141 | missing escape — `role` written raw |
| `messages_to_query/src/structure_builder.rs` | 320 | missing escape — `role` written raw (inconsistent with rest of file) |
| `messages_to_query/src/structure_builder.rs` | 245, 285 | missing escape — header/param name written raw |
| `messages_to_query/src/structure_builder.rs` | 479, 569 | missing escape (latent) — key written raw, currently only literal keys used |
| `gpt/src/fcw_chat.rs` | 6-9 | TODO + incomplete escaper (no control chars) |
| `gpt/src/fcw_tools.rs` | 8-11, 100 | TODO + duplicate incomplete escaper; unescape via `jiter` (correct) |
| `messages_to_query/src/structure_builder.rs` | most of file | existing correct implementation via `serde_json::to_string` |
| `gpt/src/handlers.rs`, `messages_to_query/src/handlers.rs` | `write_long_bytes` call sites | existing correct implementation — safe passthrough via jiter |
| `actor_runtime/src/lib.rs`, `dagops.rs` | 66-70, 102-106 | existing correct implementation via serde_json |
| `cli/src/user_input.rs` | 98-99 | non-issue — static literals |
