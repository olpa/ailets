use gpt::funcalls::{ContentItemFunction, FunCalls};

#[test]
fn single_funcall() {
    let mut funcalls = FunCalls::new();

    // Act
    funcalls.delta_index(0);
    funcalls.delta_id("call_9cFpsOXfVWMUoDz1yyyP1QXD");
    funcalls.delta_function_name("get_user_name");
    funcalls.delta_function_arguments("{}");
    funcalls.end_current();

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


#[test]
fn delta_index_regress_scenario() {
    let mut funcalls = FunCalls::new();

    // Simulate the delta_index_regress.txt scenario:
    // First tool call with index 0
    funcalls.delta_index(0);
    funcalls.delta_id("call_O8vJyvRJrH6ST1ssD97c3jPI");
    funcalls.delta_function_name("get_user_name");
    funcalls.delta_function_arguments("{}");
    funcalls.end_current();

    // Second tool call with index 1
    funcalls.delta_index(1);
    funcalls.delta_id("call_5fx8xXsKGpAhCNDTZsYoWWUx");
    funcalls.delta_function_name("get_user_name");
    funcalls.delta_function_arguments("{}");
    funcalls.end_current();

    // Assert
    let tool_calls = funcalls.get_tool_calls();
    assert_eq!(
        tool_calls,
        &vec![
            ContentItemFunction::new("call_O8vJyvRJrH6ST1ssD97c3jPI", "get_user_name", "{}"),
            ContentItemFunction::new("call_5fx8xXsKGpAhCNDTZsYoWWUx", "get_user_name", "{}")
        ]
    );
}
