use std::io::Write;
use std::rc::Rc;
use std::cell::RefCell;

use actor_runtime_mocked::RcWriter;
use dagops_mock::TrackedDagOps;
use gpt::fcw_dag::FunCallsToDag;
use gpt::fcw_trait::{FunCallResult, FunCallsWrite};
use gpt::structure_builder::StructureBuilder;

pub mod dagops_mock;

#[test]
fn basic_pass() {
    // Arrange
    let mut writer = RcWriter::new();
    let dag_writer = DummyDagWriter::new();
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Act
    builder.begin_message();
    builder.role("assistant").unwrap();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
    builder.end_message().unwrap();

    // Assert
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"text"},{"text":"hello"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn create_message_without_input_role() {
    // Arrange
    let mut writer = RcWriter::new();
    let dag_writer = DummyDagWriter::new();
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Act without "builder.role()"
    builder.begin_message();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
    builder.end_message().unwrap();

    // Assert
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"text"},{"text":"hello"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}

#[test]
fn can_call_end_message_multiple_times() {
    // Arrange
    let mut writer = RcWriter::new();
    let dag_writer = DummyDagWriter::new();
    let mut builder = StructureBuilder::new(writer.clone(), dag_writer);

    // Act
    builder.begin_message();
    builder.begin_text_chunk().unwrap();
    writer.write_all(b"hello").unwrap();
    builder.end_message().unwrap();
    builder.end_message().unwrap(); // Should be ok
    builder.end_message().unwrap(); // Should be ok

    // Assert
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"text"},{"text":"hello"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);
}

/// Simple wrapper to make Vec<u8> implement FunCallsWrite for basic tests
struct DummyDagWriter(Vec<u8>);

impl DummyDagWriter {
    fn new() -> Self {
        Self(Vec::new())
    }
}

impl Write for DummyDagWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl FunCallsWrite for DummyDagWriter {
    fn new_item(&mut self, _id: &str, _name: &str) -> FunCallResult {
        Ok(())
    }

    fn arguments_chunk(&mut self, _chunk: &str) -> FunCallResult {
        Ok(())
    }

    fn end_item(&mut self) -> FunCallResult {
        Ok(())
    }

    fn end(&mut self) -> FunCallResult {
        Ok(())
    }
}

/// Test DAG writer that maintains a single FunCallsToDag instance
struct TestDagWriter {
    tracked_dagops: Rc<RefCell<TrackedDagOps>>,
    dag_writer: Option<FunCallsToDag<'static, TrackedDagOps>>,
}

impl TestDagWriter {
    fn new_with_shared(tracked_dagops: Rc<RefCell<TrackedDagOps>>) -> Self {
        Self {
            tracked_dagops,
            dag_writer: None,
        }
    }
    
    fn ensure_dag_writer(&mut self) {
        if self.dag_writer.is_none() {
            // SAFETY: We know the TrackedDagOps will live as long as the TestDagWriter
            // because they're both owned by the test. The 'static lifetime is a lie but
            // necessary to work around Rust's lifetime system.
            let dagops_ptr = self.tracked_dagops.as_ptr();
            let dag_writer = unsafe {
                FunCallsToDag::new(&mut *dagops_ptr)
            };
            self.dag_writer = Some(dag_writer);
        }
    }
}

impl Write for TestDagWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // For Write implementation, just pretend to write
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl FunCallsWrite for TestDagWriter {
    fn new_item(&mut self, id: &str, name: &str) -> FunCallResult {
        self.ensure_dag_writer();
        self.dag_writer.as_mut().unwrap().new_item(id, name)
    }

    fn arguments_chunk(&mut self, chunk: &str) -> FunCallResult {
        self.ensure_dag_writer();
        self.dag_writer.as_mut().unwrap().arguments_chunk(chunk)
    }

    fn end_item(&mut self) -> FunCallResult {
        self.ensure_dag_writer();
        self.dag_writer.as_mut().unwrap().end_item()
    }

    fn end(&mut self) -> FunCallResult {
        self.ensure_dag_writer();
        self.dag_writer.as_mut().unwrap().end()
    }
}

#[test]
fn output_direct_tool_call() {
    // Arrange
    let writer = RcWriter::new();
    let tracked_dagops = Rc::new(RefCell::new(TrackedDagOps::default()));
    let test_dag_writer = TestDagWriter::new_with_shared(tracked_dagops.clone());
    let mut builder = StructureBuilder::new(writer.clone(), test_dag_writer);

    // Act
    {
        builder.begin_message();
        builder.tool_call_id("call_123").unwrap();
        builder.tool_call_name("get_user_name").unwrap();
        builder.tool_call_arguments_chunk("{}").unwrap();
        builder.tool_call_end_direct().unwrap();
        builder.end_message().unwrap();
    } // Ensure writers are dropped before assertions

    // Assert chat output
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"get_user_name"},{"arguments":"{}"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);

    // Get DAG writer back to check operations
    let tracked_dagops_ref = tracked_dagops.borrow();

    // Assert DAG operations - should have 2 value nodes (tool input and tool spec)
    assert_eq!(tracked_dagops_ref.value_nodes().len(), 2);

    // Assert tool input value node
    let (_, explain_tool_input, value_tool_input) =
        tracked_dagops_ref.parse_value_node(&tracked_dagops_ref.value_nodes()[0]);
    assert!(explain_tool_input.contains("tool input - get_user_name"));
    assert_eq!(value_tool_input, "{}");

    // Assert tool spec value node
    let (_, explain_tool_spec, value_tool_spec) =
        tracked_dagops_ref.parse_value_node(&tracked_dagops_ref.value_nodes()[1]);
    assert!(explain_tool_spec.contains("tool call spec - get_user_name"));
    let expected_tool_spec =
        r#"[{"type":"function","id":"call_123","name":"get_user_name"},{"arguments":"{}"}]"#;
    assert_eq!(value_tool_spec, expected_tool_spec);
}

#[test]
fn output_streaming_tool_call() {
    // Arrange
    let writer = RcWriter::new();
    let tracked_dagops = Rc::new(RefCell::new(TrackedDagOps::default()));
    let test_dag_writer = TestDagWriter::new_with_shared(tracked_dagops.clone());
    let mut builder = StructureBuilder::new(writer.clone(), test_dag_writer);

    // Act

    builder.begin_message();
    builder.tool_call_index(0).unwrap();
    builder.tool_call_id("call_123").unwrap();
    builder.tool_call_name("foo").unwrap();
    builder.tool_call_arguments_chunk("foo ").unwrap();
    builder.tool_call_arguments_chunk("args").unwrap();

    builder.tool_call_index(1).unwrap();
    builder.tool_call_id("call_456").unwrap();
    builder.tool_call_name("bar").unwrap();
    builder.tool_call_arguments_chunk("bar ").unwrap();
    builder.tool_call_arguments_chunk("args").unwrap();
    builder.end_message().unwrap();

    // Assert chat output
    let expected = r#"[{"type":"ctl"},{"role":"assistant"}]
[{"type":"function","id":"call_123","name":"foo"},{"arguments":"foo args"}]
[{"type":"function","id":"call_456","name":"bar"},{"arguments":"bar args"}]
"#
    .to_owned();
    assert_eq!(writer.get_output(), expected);

    // Get DAG writer back to check operations
    let tracked_dagops_ref = tracked_dagops.borrow();

    // Assert DAG operations - should have 4 value nodes (tool input and tool spec for each of 2 tools)
    assert_eq!(tracked_dagops_ref.value_nodes().len(), 4);

    // Assert first tool (foo) input value node
    let (_, explain_tool_input1, value_tool_input1) =
        tracked_dagops_ref.parse_value_node(&tracked_dagops_ref.value_nodes()[0]);
    assert!(explain_tool_input1.contains("tool input - foo"));
    assert_eq!(value_tool_input1, "foo args");

    // Assert first tool (foo) spec value node
    let (_, explain_tool_spec1, value_tool_spec1) =
        tracked_dagops_ref.parse_value_node(&tracked_dagops_ref.value_nodes()[1]);
    assert!(explain_tool_spec1.contains("tool call spec - foo"));
    let expected_tool_spec1 =
        r#"[{"type":"function","id":"call_123","name":"foo"},{"arguments":"foo args"}]"#;
    assert_eq!(value_tool_spec1, expected_tool_spec1);

    // Assert second tool (bar) input value node
    let (_, explain_tool_input2, value_tool_input2) =
        tracked_dagops_ref.parse_value_node(&tracked_dagops_ref.value_nodes()[2]);
    assert!(explain_tool_input2.contains("tool input - bar"));
    assert_eq!(value_tool_input2, "bar args");

    // Assert second tool (bar) spec value node
    let (_, explain_tool_spec2, value_tool_spec2) =
        tracked_dagops_ref.parse_value_node(&tracked_dagops_ref.value_nodes()[3]);
    assert!(explain_tool_spec2.contains("tool call spec - bar"));
    let expected_tool_spec2 =
        r#"[{"type":"function","id":"call_456","name":"bar"},{"arguments":"bar args"}]"#;
    assert_eq!(value_tool_spec2, expected_tool_spec2);
}
