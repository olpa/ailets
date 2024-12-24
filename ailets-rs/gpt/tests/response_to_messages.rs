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