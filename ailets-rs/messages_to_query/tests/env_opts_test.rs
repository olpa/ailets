use messages_to_query::env_opts::EnvOpts;
use std::io::Cursor;

#[test]
fn test_env_opts_happy_path() {
    let input = r#"{"foo": "bar"}"#;
    let reader = Cursor::new(input.as_bytes());

    let env_opts = EnvOpts::envopts_from_reader(reader).unwrap();

    let foo_value = env_opts.get("foo").unwrap();
    assert_eq!(foo_value.as_str().unwrap(), "bar");
}

#[test]
fn test_env_opts_not_map() {
    let input = r#"["not", "a", "map"]"#;
    let reader = Cursor::new(input.as_bytes());

    let result = EnvOpts::envopts_from_reader(reader);
    assert!(result.is_err());
}
#[test]
fn test_env_opts_invalid_json() {
    let input = r#"{"foo": "bar""#; // Missing closing brace
    let reader = Cursor::new(input.as_bytes());

    let result = EnvOpts::envopts_from_reader(reader);
    assert!(result.is_err());
}
