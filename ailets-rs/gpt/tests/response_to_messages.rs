use gpt::process_gpt;

mod mocked_node_runtime;
use mocked_node_runtime::{clear_mocks, get_output, set_input};

#[test]
fn test_basic_processing() {
    clear_mocks();
    let input = "Hello GPT!";
    set_input(&[input]);

    process_gpt();

    assert_eq!(get_output(), input);
}

#[test]
fn test_basic_streaming() {
    clear_mocks();
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_streaming.txt")
        .expect("Failed to read fixture file");
    set_input(&[&fixture_content]);

    process_gpt();

    assert_eq!(get_output(), fixture_content);
} 