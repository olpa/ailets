use gpt::funcalls::{ContentItemFunction, FunCalls};

#[test]
fn single_funcall_direct() {
    let mut funcalls = FunCalls::new();

    // Act
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
fn several_funcalls_direct() {
    let mut funcalls = FunCalls::new();

    // First tool call with index 0
    funcalls.delta_id("call_foo");
    funcalls.delta_function_name("get_foo");
    funcalls.delta_function_arguments("{foo_args}");
    funcalls.end_current();

    // Second tool call with index 1
    funcalls.delta_id("call_bar");
    funcalls.delta_function_name("get_bar");
    funcalls.delta_function_arguments("{bar_args}");
    funcalls.end_current();

    // Third tool call with index 2
    funcalls.delta_id("call_baz");
    funcalls.delta_function_name("get_baz");
    funcalls.delta_function_arguments("{baz_args}");
    funcalls.end_current();

    // Assert
    let tool_calls = funcalls.get_tool_calls();
    assert_eq!(
        tool_calls,
        &vec![
            ContentItemFunction::new("call_foo", "get_foo", "{foo_args}"),
            ContentItemFunction::new("call_bar", "get_bar", "{bar_args}"),
            ContentItemFunction::new("call_baz", "get_baz", "{baz_args}"),
        ]
    );
}
