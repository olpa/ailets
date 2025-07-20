use gpt::funcalls::{ContentItemFunction, FunCalls};

//
// "Happy path" style tests
//

// Terminology and differences:
// - "Direct" funcalls: without using "index", using "end_current" to finalize
// - "Streaming" funcalls: using "index" to indicate progress

#[test]
fn single_funcall_direct() {
    // Arrange
    let mut writer = Vec::new();
    let mut funcalls = FunCalls::new();

    // Act
    // Don't call "index"
    funcalls
        .id("call_9cFpsOXfVWMUoDz1yyyP1QXD", &mut writer)
        .unwrap();
    funcalls.name("get_user_name", &mut writer).unwrap();
    funcalls.arguments_chunk("{}", &mut writer).unwrap();
    funcalls.end_current(&mut writer).unwrap();

    // Assert
    funcalls.end(&mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_9cFpsOXfVWMUoDz1yyyP1QXD","name":"get_user_name"},{"arguments":"{}"}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn several_funcalls_direct() {
    // Arrange
    let mut writer = Vec::new();
    let mut funcalls = FunCalls::new();

    // First tool call - Don't call "index"
    funcalls.id("call_foo", &mut writer).unwrap();
    funcalls.name("get_foo", &mut writer).unwrap();
    funcalls.arguments_chunk("{foo_args}", &mut writer).unwrap();
    funcalls.end_current(&mut writer).unwrap();

    // Second tool call - Don't call "index"
    funcalls.id("call_bar", &mut writer).unwrap();
    funcalls.name("get_bar", &mut writer).unwrap();
    funcalls.arguments_chunk("{bar_args}", &mut writer).unwrap();
    funcalls.end_current(&mut writer).unwrap();

    // Third tool call - Don't call "index"
    funcalls.id("call_baz", &mut writer).unwrap();
    funcalls.name("get_baz", &mut writer).unwrap();
    funcalls.arguments_chunk("{baz_args}", &mut writer).unwrap();
    funcalls.end_current(&mut writer).unwrap();

    // Assert
    funcalls.end(&mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_foo","name":"get_foo"},{"arguments":"{foo_args}"}]
[{"type":"function","id":"call_bar","name":"get_bar"},{"arguments":"{bar_args}"}]
[{"type":"function","id":"call_baz","name":"get_baz"},{"arguments":"{baz_args}"}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn single_element_streaming() {
    // Arrange
    let mut writer = Vec::new();
    let mut funcalls = FunCalls::new();

    // Act - streaming mode with delta_index
    funcalls.index(0, &mut writer).unwrap();

    funcalls
        .id("call_9cFpsOXfVWMUoDz1yyyP1QXD", &mut writer)
        .unwrap();
    funcalls.name("get_user_name", &mut writer).unwrap();
    funcalls.arguments_chunk("{}", &mut writer).unwrap();
    funcalls.end_current(&mut writer).unwrap();

    // Assert
    funcalls.end(&mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_9cFpsOXfVWMUoDz1yyyP1QXD","name":"get_user_name"},{"arguments":"{}"}]
"#;
    assert_eq!(output, expected);
}

#[test]
fn several_elements_streaming() {
    // Arrange
    let mut writer = Vec::new();
    let mut funcalls = FunCalls::new();

    // Act - streaming mode with delta_index, multiple elements in one round
    funcalls.index(0, &mut writer).unwrap();

    funcalls.id("call_foo", &mut writer).unwrap();
    funcalls.name("get_foo", &mut writer).unwrap();
    funcalls.arguments_chunk("{foo_args}", &mut writer).unwrap();

    funcalls.index(1, &mut writer).unwrap();

    funcalls.id("call_bar", &mut writer).unwrap();
    funcalls.name("get_bar", &mut writer).unwrap();
    funcalls.arguments_chunk("{bar_args}", &mut writer).unwrap();

    funcalls.index(2, &mut writer).unwrap();

    funcalls.id("call_baz", &mut writer).unwrap();
    funcalls.name("get_baz", &mut writer).unwrap();
    funcalls.arguments_chunk("{baz_args}", &mut writer).unwrap();

    // Assert
    funcalls.end(&mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_foo","name":"get_foo"},{"arguments":"{foo_args}"}]
[{"type":"function","id":"call_bar","name":"get_bar"},{"arguments":"{bar_args}"}]
[{"type":"function","id":"call_baz","name":"get_baz"},{"arguments":"{baz_args}"}]
"#;
    assert_eq!(output, expected);
}

//
// More detailed tests
//

#[test]
fn index_increment_validation() {
    let mut funcalls = FunCalls::new();

    // First index must be 0
    assert!(funcalls.delta_index(0).is_ok());

    // Index can stay the same
    assert!(funcalls.delta_index(0).is_ok());

    // Index can increment by 1
    assert!(funcalls.delta_index(1).is_ok());

    // Index can stay the same
    assert!(funcalls.delta_index(1).is_ok());

    // Index can increment by 1
    assert!(funcalls.delta_index(2).is_ok());

    // Index cannot skip
    let result = funcalls.delta_index(4);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("cannot skip values"));

    // Index cannot go backwards (never decreases)
    let result = funcalls.delta_index(1);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("cannot decrease"));
}

#[test]
fn first_index_must_be_zero() {
    let mut funcalls = FunCalls::new();

    // First index must be 0
    let result = funcalls.delta_index(1);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("First tool call index must be 0"));
}

#[test]
fn id_field_only_once() {
    let mut funcalls = FunCalls::new();

    // Enable streaming mode
    assert!(funcalls.delta_index(0).is_ok());

    // First ID set should work
    assert!(funcalls.delta_id("call_123").is_ok());

    // Second ID set in same streaming session should fail
    let result = funcalls.delta_id("_extra");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("ID field cannot be set multiple times"));
}

#[test]
fn name_field_only_once() {
    let mut funcalls = FunCalls::new();

    // Enable streaming mode
    assert!(funcalls.delta_index(0).is_ok());

    // First name set should work
    assert!(funcalls.delta_function_name("get_user").is_ok());

    // Second name set in same streaming session should fail
    let result = funcalls.delta_function_name("_plus");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .contains("Function name field cannot be set multiple times"));
}

#[test]
fn arguments_span_multiple_deltas() {
    let mut funcalls = FunCalls::new();

    // Enable streaming mode
    assert!(funcalls.delta_index(0).is_ok());

    // Arguments can be set multiple times - this should work
    funcalls.delta_function_arguments("{");
    funcalls.delta_function_arguments("\"arg\": \"value\"");
    funcalls.delta_function_arguments("}");

    // No error should occur - arguments are allowed to span deltas
}
