use gpt::process_gpt;

mod mocked_node_runtime;
use mocked_node_runtime::{clear_mocks, get_output, set_input};

#[test]
fn test_basic_processing() {
    clear_mocks();
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_reponse.txt")
        .expect("Failed to read fixture file 'basic_reponse.txt'");
    set_input(&[&fixture_content]);

    process_gpt();

    assert_eq!(get_output(), fixture_content);
}

#[test]
fn test_basic_streaming() {
    clear_mocks();
    let fixture_content = std::fs::read_to_string("tests/fixture/basic_streaming.txt")
        .expect("Failed to read fixture file 'basic_streaming.txt'");
    set_input(&[&fixture_content]);

    process_gpt();

    assert_eq!(get_output(), fixture_content);
} 
