use actor_runtime_mocked::RcWriter;
use gpt::_process_gpt;
pub mod dagops_mock;
use dagops_mock::TrackedInjectDagOps;
use gpt::funcalls::ContentItemFunction;
use std::io::Cursor;

fn get_expected_basic_message() -> String {
    "{\"type\":\"ctl\",\"role\":\"assistant\"}\n[{\"type\":\"text\"},{\"text\":\"Hello! How can I assist you today?\"}]\n"
        .to_string()
}

#[test]
fn test_basic_processing() {
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_response.txt")
        .expect("Failed to read fixture file 'basic_response.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();

    _process_gpt(reader, writer.clone(), &mut TrackedInjectDagOps::new()).unwrap();

    assert_eq!(writer.get_output(), get_expected_basic_message());
}

#[test]
fn test_streaming() {
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_streaming.txt")
        .expect("Failed to read fixture file 'basic_streaming.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();

    _process_gpt(reader, writer.clone(), &mut TrackedInjectDagOps::new()).unwrap();

    assert_eq!(writer.get_output(), get_expected_basic_message());
}

#[test]
fn funcall_response() {
    let fixture_content = std::fs::read_to_string("tests/fixture/funcall_response.txt")
        .expect("Failed to read fixture file 'funcall_response.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();
    let mut dagops = TrackedInjectDagOps::new();

    _process_gpt(reader, writer.clone(), &mut dagops).unwrap();

    assert_eq!(writer.get_output(), "");
    assert_eq!(
        dagops.get_tool_calls(),
        &[ContentItemFunction::new(
            "call_9br5e3keEQrjl49h7lteRxW4",
            "get_user_name",
            "{}"
        )]
    );
}
#[test]
fn funcall_streaming() {
    let fixture_content = std::fs::read_to_string("tests/fixture/funcall_streaming.txt")
        .expect("Failed to read fixture file 'funcall_streaming.txt'");
    let reader = Cursor::new(fixture_content);
    let writer = RcWriter::new();
    let mut dagops = TrackedInjectDagOps::new();

    _process_gpt(reader, writer.clone(), &mut dagops).unwrap();

    assert_eq!(writer.get_output(), "");
    assert_eq!(
        dagops.get_tool_calls(),
        &[ContentItemFunction::new(
            "call_9cFpsOXfVWMUoDz1yyyP1QXD",
            "get_user_name",
            "{}"
        )]
    );
}
