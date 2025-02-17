use actor_runtime_mocked::{clear_mocks, get_file};
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

    let result = String::from_utf8(get_file("").unwrap()).unwrap();
    assert_eq!(result, get_expected_basic_message());
}

#[test]
fn test_streaming() {
    clear_mocks();
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_streaming.txt")
        .expect("Failed to read fixture file 'basic_streaming.txt'");
    let reader = Cursor::new(fixture_content);

    _process_gpt(reader);

    let result = String::from_utf8(get_file("").unwrap()).unwrap();
    assert_eq!(result, get_expected_basic_message());
}
