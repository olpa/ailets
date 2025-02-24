use gpt::funcall::{ContentItemFunction, FunCalls, FunCallsTrait};

#[test]
fn single_funcall() {
    let mut funcalls = FunCalls::new();

    // Start first round
    funcalls.start_delta_round();

    // Add one function call
    funcalls.start_delta();

    // Set the function call details
    funcalls.delta_id("call_9cFpsOXfVWMUoDz1yyyP1QXD".to_string());
    funcalls.delta_function_name("get_user_name".to_string());
    funcalls.delta_function_arguments("{}".to_string());

    // Get and verify results
    let tool_calls = funcalls.get_tool_calls();
    assert_eq!(tool_calls.len(), 1);

    let call = &tool_calls[0];
    assert_eq!(
        call,
        &ContentItemFunction::new(
            "call_9cFpsOXfVWMUoDz1yyyP1QXD".to_string(),
            "get_user_name".to_string(),
            "{}".to_string(),
        )
    );
}
