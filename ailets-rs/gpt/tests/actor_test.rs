use actor_runtime_mocked::RcWriter;
use gpt::_process_gpt;
pub mod dagops_mock;
use dagops_mock::TrackedDagOps;
use std::io::Cursor;

fn get_expected_basic_message() -> String {
    "[{\"type\":\"ctl\"},{\"role\":\"assistant\"}]\n[{\"type\":\"text\"},{\"text\":\"Hello! How can I assist you today?\"}]\n"
        .to_string()
}

#[test]
fn test_basic_processing() {
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_response.txt")
        .expect("Failed to read fixture file 'basic_response.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();

    _process_gpt(reader, writer.clone(), &mut TrackedDagOps::default()).unwrap();

    assert_eq!(writer.get_output(), get_expected_basic_message());
}

#[test]
fn test_streaming() {
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_streaming.txt")
        .expect("Failed to read fixture file 'basic_streaming.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();

    _process_gpt(reader, writer.clone(), &mut TrackedDagOps::default()).unwrap();

    assert_eq!(writer.get_output(), get_expected_basic_message());
}

#[test]
fn funcall_response() {
    let fixture_content = std::fs::read_to_string("tests/fixture/funcall_response.txt")
        .expect("Failed to read fixture file 'funcall_response.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();
    let mut dagops = TrackedDagOps::default();

    _process_gpt(reader, writer.clone(), &mut dagops).unwrap();

    // Assert chat output
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_9br5e3keEQrjl49h7lteRxW4","name":"get_user_name"},{"arguments":"{}"}]
"#;
    assert_eq!(writer.get_output(), expected);
    
    // Assert DAG operations - should have 2 value nodes (tool input and tool spec)
    assert_eq!(dagops.value_nodes().len(), 2);
    
    // Assert tool input value node
    let (_, explain_tool_input, value_tool_input) =
        dagops.parse_value_node(&dagops.value_nodes()[0]);
    assert!(explain_tool_input.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input, "{}");
    
    // Assert tool spec value node
    let (_, explain_tool_spec, value_tool_spec) =
        dagops.parse_value_node(&dagops.value_nodes()[1]);
    assert!(explain_tool_spec.contains("tool call spec - get_user_name"));
    let expected_tool_spec =
        r#"[{"type":"function","id":"call_9br5e3keEQrjl49h7lteRxW4","name":"get_user_name"},{"arguments":"{}"}]"#;
    assert_eq!(value_tool_spec, expected_tool_spec);
}

#[test]
fn funcall_streaming() {
    let fixture_content = std::fs::read_to_string("tests/fixture/funcall_streaming.txt")
        .expect("Failed to read fixture file 'funcall_streaming.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();
    let mut dagops = TrackedDagOps::default();

    _process_gpt(reader, writer.clone(), &mut dagops).unwrap();

    // Assert chat output
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_9cFpsOXfVWMUoDz1yyyP1QXD","name":"get_user_name"},{"arguments":""}]
"}]
"#;
    assert_eq!(writer.get_output(), expected);
    
    // Assert DAG operations - should have 2 value nodes (tool input and tool spec)
    assert_eq!(dagops.value_nodes().len(), 2);
    
    // Assert tool input value node
    let (_, explain_tool_input, value_tool_input) =
        dagops.parse_value_node(&dagops.value_nodes()[0]);
    assert!(explain_tool_input.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input, "");
    
    // Assert tool spec value node
    let (_, explain_tool_spec, value_tool_spec) =
        dagops.parse_value_node(&dagops.value_nodes()[1]);
    assert!(explain_tool_spec.contains("tool call spec - get_user_name"));
    let expected_tool_spec =
        r#"[{"type":"function","id":"call_9cFpsOXfVWMUoDz1yyyP1QXD","name":"get_user_name"},{"arguments":""}]"#;
    assert_eq!(value_tool_spec, expected_tool_spec);
}

#[test]
fn delta_index_regress() {
    let fixture_content = std::fs::read_to_string("tests/fixture/delta_index_regress.txt")
        .expect("Failed to read fixture file 'delta_index_regress.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();
    let mut dagops = TrackedDagOps::default();

    _process_gpt(reader, writer.clone(), &mut dagops).unwrap();

    // Assert chat output - should have 2 function calls
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_O8vJyvRJrH6ST1ssD97c3jPI","name":"get_user_name"},{"arguments":""}]
"}]
[{"type":"function","id":"call_5fx8xXsKGpAhCNDTZsYoWWUx","name":"get_user_name"},{"arguments":""}]
"}]
"#;
    assert_eq!(writer.get_output(), expected);
    
    // Assert DAG operations - should have 4 value nodes (tool input and tool spec for each of 2 tools)
    assert_eq!(dagops.value_nodes().len(), 4);
    
    // Assert first tool input value node
    let (_, explain_tool_input1, value_tool_input1) =
        dagops.parse_value_node(&dagops.value_nodes()[0]);
    assert!(explain_tool_input1.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input1, "");
    
    // Assert first tool spec value node
    let (_, explain_tool_spec1, value_tool_spec1) =
        dagops.parse_value_node(&dagops.value_nodes()[1]);
    assert!(explain_tool_spec1.contains("tool call spec - get_user_name"));
    let expected_tool_spec1 =
        r#"[{"type":"function","id":"call_O8vJyvRJrH6ST1ssD97c3jPI","name":"get_user_name"},{"arguments":""}]"#;
    assert_eq!(value_tool_spec1, expected_tool_spec1);
    
    // Assert second tool input value node
    let (_, explain_tool_input2, value_tool_input2) =
        dagops.parse_value_node(&dagops.value_nodes()[2]);
    assert!(explain_tool_input2.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input2, "");
    
    // Assert second tool spec value node
    let (_, explain_tool_spec2, value_tool_spec2) =
        dagops.parse_value_node(&dagops.value_nodes()[3]);
    assert!(explain_tool_spec2.contains("tool call spec - get_user_name"));
    let expected_tool_spec2 =
        r#"[{"type":"function","id":"call_5fx8xXsKGpAhCNDTZsYoWWUx","name":"get_user_name"},{"arguments":""}]"#;
    assert_eq!(value_tool_spec2, expected_tool_spec2);
}
