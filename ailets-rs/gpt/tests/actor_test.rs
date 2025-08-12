use actor_runtime_mocked::RcWriter;
use gpt::_process_gpt;
pub mod dagops_mock;
use dagops_mock::TrackedDagOps;
use std::io::Cursor;

// Helper function to get chat output from DAG value nodes
fn get_chat_output(tracked_dagops: &TrackedDagOps) -> String {
    let value_nodes = tracked_dagops.value_nodes();
    assert!(
        !value_nodes.is_empty(),
        "Expected at least one value node for chat output"
    );
    let first_node = &value_nodes[0];
    let (_, _, chat_output) = tracked_dagops.parse_value_node(first_node);
    chat_output
}

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

    _process_gpt(reader, writer.clone(), TrackedDagOps::default()).unwrap();

    assert_eq!(writer.get_output(), get_expected_basic_message());
}

#[test]
fn test_streaming() {
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_streaming.txt")
        .expect("Failed to read fixture file 'basic_streaming.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();

    _process_gpt(reader, writer.clone(), TrackedDagOps::default()).unwrap();

    assert_eq!(writer.get_output(), get_expected_basic_message());
}

#[test]
fn funcall_response() {
    let fixture_content = std::fs::read_to_string("tests/fixture/funcall_response.txt")
        .expect("Failed to read fixture file 'funcall_response.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();
    let dagops = TrackedDagOps::default();

    _process_gpt(reader, writer.clone(), dagops.clone()).unwrap();

    // Assert stdout is empty since there was no text content
    assert_eq!(writer.get_output(), "");

    // Assert chat output (including function call) is in DAG value nodes
    let chat_output = get_chat_output(&dagops);
    let expected_chat = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_9br5e3keEQrjl49h7lteRxW4","name":"get_user_name"},{"arguments":"{}"}]
"#;
    assert_eq!(chat_output, expected_chat);

    // Assert DAG operations - should have 3 value nodes (chat output + tool input + tool spec)
    assert_eq!(dagops.value_nodes().len(), 3);

    // Assert tool input value node (index 1 since chat output is at index 0)
    let (_, explain_tool_input, value_tool_input) =
        dagops.parse_value_node(&dagops.value_nodes()[1]);
    assert!(explain_tool_input.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input, "{}");

    // Assert tool spec value node (index 2)
    let (_, explain_tool_spec, value_tool_spec) = dagops.parse_value_node(&dagops.value_nodes()[2]);
    assert!(explain_tool_spec.contains("tool call spec - get_user_name"));
    let expected_tool_spec = r#"[{"type":"function","id":"call_9br5e3keEQrjl49h7lteRxW4","name":"get_user_name"},{"arguments":"{}"}]"#;
    assert_eq!(value_tool_spec, expected_tool_spec);

    // Assert that the workflows include .gpt workflow
    let workflows = dagops.workflows();
    let gpt_workflow_exists = workflows.iter().any(|workflow| {
        let (_, workflow_name, _) = dagops.parse_workflow(workflow);
        workflow_name == ".gpt"
    });
    assert!(
        gpt_workflow_exists,
        "Expected .gpt workflow to be added to DAG"
    );
}

#[test]
fn funcall_streaming() {
    let fixture_content = std::fs::read_to_string("tests/fixture/funcall_streaming.txt")
        .expect("Failed to read fixture file 'funcall_streaming.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();
    let dagops = TrackedDagOps::default();

    _process_gpt(reader, writer.clone(), dagops.clone()).unwrap();

    // Assert stdout is empty since there was no text content
    assert_eq!(writer.get_output(), "");

    // Assert chat output (including function call) is in DAG value nodes
    let chat_output = get_chat_output(&dagops);
    let expected_chat = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_9cFpsOXfVWMUoDz1yyyP1QXD","name":"get_user_name"},{"arguments":"{}"}]
"#;
    assert_eq!(chat_output, expected_chat);

    // Assert DAG operations - should have 3 value nodes (chat output + tool input + tool spec)
    assert_eq!(dagops.value_nodes().len(), 3);

    // Assert tool input value node (index 1 since chat output is at index 0)
    let (_, explain_tool_input, value_tool_input) =
        dagops.parse_value_node(&dagops.value_nodes()[1]);
    assert!(explain_tool_input.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input, "{}");

    // Assert tool spec value node (index 2)
    let (_, explain_tool_spec, value_tool_spec) = dagops.parse_value_node(&dagops.value_nodes()[2]);
    assert!(explain_tool_spec.contains("tool call spec - get_user_name"));
    let expected_tool_spec = r#"[{"type":"function","id":"call_9cFpsOXfVWMUoDz1yyyP1QXD","name":"get_user_name"},{"arguments":"{}"}]"#;
    assert_eq!(value_tool_spec, expected_tool_spec);

    // Assert that the workflows include .gpt workflow
    let workflows = dagops.workflows();
    let gpt_workflow_exists = workflows.iter().any(|workflow| {
        let (_, workflow_name, _) = dagops.parse_workflow(workflow);
        workflow_name == ".gpt"
    });
    assert!(
        gpt_workflow_exists,
        "Expected .gpt workflow to be added to DAG"
    );
}

#[test]
fn delta_index_regress() {
    let fixture_content = std::fs::read_to_string("tests/fixture/delta_index_regress.txt")
        .expect("Failed to read fixture file 'delta_index_regress.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();
    let dagops = TrackedDagOps::default();

    _process_gpt(reader, writer.clone(), dagops.clone()).unwrap();

    // Assert stdout is empty since there was no text content
    assert_eq!(writer.get_output(), "");

    // Assert chat output (including function calls) is in DAG value nodes
    let chat_output = get_chat_output(&dagops);
    let expected_chat = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_O8vJyvRJrH6ST1ssD97c3jPI","name":"get_user_name"},{"arguments":"{}"}]
[{"type":"function","id":"call_5fx8xXsKGpAhCNDTZsYoWWUx","name":"get_user_name"},{"arguments":"{}"}]
"#;
    assert_eq!(chat_output, expected_chat);

    // Assert DAG operations - should have 5 value nodes (chat output + tool input and tool spec for each of 2 tools)
    assert_eq!(dagops.value_nodes().len(), 5);

    // Assert first tool input value node (index 1 since chat output is at index 0)
    let (_, explain_tool_input1, value_tool_input1) =
        dagops.parse_value_node(&dagops.value_nodes()[1]);
    assert!(explain_tool_input1.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input1, "{}");

    // Assert first tool spec value node (index 2)
    let (_, explain_tool_spec1, value_tool_spec1) =
        dagops.parse_value_node(&dagops.value_nodes()[2]);
    assert!(explain_tool_spec1.contains("tool call spec - get_user_name"));
    let expected_tool_spec1 = r#"[{"type":"function","id":"call_O8vJyvRJrH6ST1ssD97c3jPI","name":"get_user_name"},{"arguments":"{}"}]"#;
    assert_eq!(value_tool_spec1, expected_tool_spec1);

    // Assert second tool input value node (index 3)
    let (_, explain_tool_input2, value_tool_input2) =
        dagops.parse_value_node(&dagops.value_nodes()[3]);
    assert!(explain_tool_input2.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input2, "{}");

    // Assert second tool spec value node (index 4)
    let (_, explain_tool_spec2, value_tool_spec2) =
        dagops.parse_value_node(&dagops.value_nodes()[4]);
    assert!(explain_tool_spec2.contains("tool call spec - get_user_name"));
    let expected_tool_spec2 = r#"[{"type":"function","id":"call_5fx8xXsKGpAhCNDTZsYoWWUx","name":"get_user_name"},{"arguments":"{}"}]"#;
    assert_eq!(value_tool_spec2, expected_tool_spec2);

    // Assert that the workflows include .gpt workflow
    let workflows = dagops.workflows();
    let gpt_workflow_exists = workflows.iter().any(|workflow| {
        let (_, workflow_name, _) = dagops.parse_workflow(workflow);
        workflow_name == ".gpt"
    });
    assert!(
        gpt_workflow_exists,
        "Expected .gpt workflow to be added to DAG"
    );
}

#[test]
fn duplicate_tool_call_id_error() {
    let fixture_content = std::fs::read_to_string("tests/fixture/funcall_duplicate_name.txt")
        .expect("Failed to read fixture file 'funcall_duplicate_name.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();
    let dagops = TrackedDagOps::default();

    let result = _process_gpt(reader, writer.clone(), dagops.clone());

    assert!(result.is_err());
    let error_message = result.unwrap_err().to_string();
    assert!(error_message.contains("ID is already given"));
}

#[test]
fn nonincremental_index_error() {
    let fixture_content = std::fs::read_to_string("tests/fixture/funcall_nonincremental_index.txt")
        .expect("Failed to read fixture file 'funcall_nonincremental_index.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();
    let dagops = TrackedDagOps::default();

    let result = _process_gpt(reader, writer.clone(), dagops.clone());

    assert!(result.is_err());
    let error_message = result.unwrap_err().to_string();
    assert!(error_message.contains("Tool call index cannot decrease"));
}

#[test]
fn arguments_before_name_retained() {
    let fixture_content = std::fs::read_to_string("tests/fixture/arguments_before_name.txt")
        .expect("Failed to read fixture file 'arguments_before_name.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();
    let dagops = TrackedDagOps::default();

    _process_gpt(reader, writer.clone(), dagops.clone()).unwrap();

    // Assert stdout is empty since there was no text content
    assert_eq!(writer.get_output(), "");

    // Assert chat output (including function call) is in DAG value nodes
    let chat_output = get_chat_output(&dagops);
    let expected_chat = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_test123","name":"test_function"},{"arguments":"{\"param\": true, \"value\": 42}"}]
"#;
    assert_eq!(chat_output, expected_chat);

    // Assert DAG operations - should have 3 value nodes (chat output + tool input + tool spec)
    assert_eq!(dagops.value_nodes().len(), 3);

    // Assert tool input value node (index 1 since chat output is at index 0)
    let (_, explain_tool_input, value_tool_input) =
        dagops.parse_value_node(&dagops.value_nodes()[1]);
    assert!(explain_tool_input.contains("tool input - test_function"));
    assert_eq!(value_tool_input, r#"{"param": true, "value": 42}"#);

    // Assert tool spec value node (index 2)
    let (_, explain_tool_spec, value_tool_spec) = dagops.parse_value_node(&dagops.value_nodes()[2]);
    assert!(explain_tool_spec.contains("tool call spec - test_function"));
    let expected_tool_spec = r#"[{"type":"function","id":"call_test123","name":"test_function"},{"arguments":"{\"param\": true, \"value\": 42}"}]"#;
    assert_eq!(value_tool_spec, expected_tool_spec);

    // Assert that the workflows include .gpt workflow
    let workflows = dagops.workflows();
    let gpt_workflow_exists = workflows.iter().any(|workflow| {
        let (_, workflow_name, _) = dagops.parse_workflow(workflow);
        workflow_name == ".gpt"
    });
    assert!(
        gpt_workflow_exists,
        "Expected .gpt workflow to be added to DAG"
    );
}
