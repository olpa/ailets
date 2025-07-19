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

#[test]
fn single_element_streaming_one_round() {
    let mut funcalls = FunCalls::new();

    // Act - streaming mode with delta_index
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
fn single_element_streaming_several_rounds() {
    let mut funcalls = FunCalls::new();

    // Act - streaming mode with delta_index, multiple rounds
    // Round 1: Start building the element
    funcalls.delta_index(0);
    funcalls.delta_id("call_9cFps");
    funcalls.delta_function_name("get_user");
    funcalls.end_current();

    // Round 2: Continue building the same element (accumulate)
    funcalls.delta_id("call_9cFpsOXfVWMUoDz1yyyP1QXD");
    funcalls.delta_function_name("get_user_name");
    funcalls.delta_index(0);
    funcalls.delta_function_arguments("{}");
    funcalls.end_current();

    // Assert
    let tool_calls = funcalls.get_tool_calls();
    assert_eq!(
        tool_calls,
        &vec![ContentItemFunction::new(
            "call_9cFpscall_9cFpsOXfVWMUoDz1yyyP1QXD",
            "get_userget_user_name",
            "{}",
        )]
    );
}

#[test]
fn several_elements_streaming_one_round() {
    let mut funcalls = FunCalls::new();

    // Act - streaming mode with delta_index, multiple elements in one round
    funcalls.delta_index(0);
    funcalls.delta_id("call_foo");
    funcalls.delta_function_name("get_foo");
    funcalls.delta_function_arguments("{foo_args}");
    funcalls.end_current();

    funcalls.delta_id("call_bar");
    funcalls.delta_function_name("get_bar");
    funcalls.delta_function_arguments("{bar_args}");
    funcalls.delta_index(1);
    funcalls.end_current();

    funcalls.delta_index(2);
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

#[test]
fn several_elements_streaming_several_rounds() {
    let mut funcalls = FunCalls::new();

    // Act - streaming mode with valid index progression (only same or increment)
    // Tool call 0: Initial data
    funcalls.delta_index(0);
    funcalls.delta_id("call_foo");
    funcalls.delta_function_name("get_foo");
    funcalls.end_current();

    // Tool call 0: More arguments (same index)
    funcalls.delta_index(0);
    funcalls.delta_function_arguments("{foo_args}");
    funcalls.end_current();

    // Tool call 1: Initial data
    funcalls.delta_index(1);
    funcalls.delta_id("call_bar");
    funcalls.delta_function_name("get_bar");
    funcalls.end_current();

    // Tool call 1: More arguments (same index)
    funcalls.delta_index(1);
    funcalls.delta_function_arguments("{bar_args}");
    funcalls.end_current();

    // Tool call 2: Complete data
    funcalls.delta_index(2);
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
