use gpt::process_gpt;

mod mocked_node_runtime;
use mocked_node_runtime::{clear_mocks, get_output, set_input};

fn get_expected_basic_message() -> String {
    "{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\
    \"Hello! How can I assist you today?\"}]}\n"
        .to_string()
}

#[test]
fn test_basic_processing() {
    clear_mocks();
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_reponse.txt")
        .expect("Failed to read fixture file 'basic_reponse.txt'");
    set_input(&[&fixture_content]);

    process_gpt();

    assert_eq!(get_output(), get_expected_basic_message());
}

#[test]
fn test_streaming() {
    clear_mocks();
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_streaming.txt")
        .expect("Failed to read fixture file 'basic_streaming.txt'");
    set_input(&[&fixture_content]);

    process_gpt();

    assert_eq!(get_output(), get_expected_basic_message());
}
