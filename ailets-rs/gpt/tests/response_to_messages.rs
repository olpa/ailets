use areader::mocked_actor_runtime::{clear_mocks, get_output};
use gpt::_process_gpt;
use std::io::Cursor;

fn get_expected_basic_message() -> String {
    "{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\
    \"Hello! How can I assist you today?\"}]}\n"
        .to_string()
}

#[test]
fn test_basic_processing() {
    clear_mocks();
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_response.txt")
        .expect("Failed to read fixture file 'basic_response.txt'");
    let reader = Cursor::new(fixture_content);

    _process_gpt(reader);

    assert_eq!(get_output(), get_expected_basic_message());
}

#[test]
fn test_streaming() {
    clear_mocks();
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_streaming.txt")
        .expect("Failed to read fixture file 'basic_streaming.txt'");
    let reader = Cursor::new(fixture_content);

    _process_gpt(reader);

    assert_eq!(get_output(), get_expected_basic_message());
}
