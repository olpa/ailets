use gpt::funcall::{ContentItemFunction, FunCalls, FunCallsTrait};

#[test]
fn single_funcall() {
    let mut funcalls = FunCalls::new();

    // Act
    funcalls.start_delta_round();
    funcalls.start_delta();
    funcalls.delta_id("call_9cFpsOXfVWMUoDz1yyyP1QXD").unwrap();
    funcalls.delta_function_name("get_user_name").unwrap();
    funcalls.delta_function_arguments("{}").unwrap();

    // Assert
    let tool_calls = funcalls.get_tool_calls();
    assert_eq!(
        tool_calls,
        &vec![ContentItemFunction::new(
            "call_9cFpsOXfVWMUoDz1yyyP1QXD",
            "get_user_name",
            "{}",
        )]
    );
}
