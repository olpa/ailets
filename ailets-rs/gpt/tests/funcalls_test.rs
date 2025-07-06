use gpt::funcalls::{ContentItemFunction, FunCalls};

#[test]
fn single_funcall() {
    let mut funcalls = FunCalls::new();

    // Act
    funcalls.delta_index(0).unwrap();
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

#[test]
fn check_index() {
    // Arrange
    let mut funcalls = FunCalls::new();

    // Act
    assert!(funcalls.delta_index(0).is_ok());
    assert!(funcalls.delta_index(1).is_ok()); // Now we can set any index
}

#[test]
fn delta_appends() {
    // Arrange
    let mut funcalls = FunCalls::new();

    // Act
    funcalls.delta_index(0).unwrap();
    funcalls.delta_id("call_1").unwrap();
    funcalls.delta_function_name("func1").unwrap();
    funcalls.delta_function_arguments("{}").unwrap();

    funcalls.delta_id("call_2").unwrap();
    funcalls.delta_function_name("func2").unwrap();
    funcalls.delta_function_arguments("{\"param\":2}").unwrap();

    // Assert
    let tool_calls = funcalls.get_tool_calls();
    assert_eq!(
        tool_calls,
        &vec![ContentItemFunction::new(
            "call_1call_2",
            "func1func2",
            "{}{\"param\":2}"
        )]
    );
}

#[test]
fn several_rounds_with_delta() {
    let mut funcalls = FunCalls::new();

    // Act: first round
    funcalls.delta_index(0).unwrap();
    funcalls.delta_id("call_1").unwrap();
    funcalls.delta_function_name("func1").unwrap();
    funcalls.delta_function_arguments("{}").unwrap();
    funcalls.delta_index(1).unwrap();
    funcalls.delta_id("call_2").unwrap();
    funcalls.delta_function_name("func2").unwrap();
    funcalls.delta_function_arguments("{\"param\":2}").unwrap();

    // Act: second round - append to existing items
    funcalls.delta_index(0).unwrap();
    funcalls.delta_id("++").unwrap();
    funcalls.delta_function_name("++").unwrap();
    funcalls.delta_function_arguments("++").unwrap();
    funcalls.delta_index(1).unwrap();
    funcalls.delta_id("==").unwrap();
    funcalls.delta_function_name("==").unwrap();
    funcalls.delta_function_arguments("==").unwrap();

    // Assert
    let tool_calls = funcalls.get_tool_calls();
    assert_eq!(
        tool_calls,
        &vec![
            ContentItemFunction::new("call_1++", "func1++", "{}++"),
            ContentItemFunction::new("call_2==", "func2==", "{\"param\":2}=="),
        ]
    );
}

#[test]
fn start_delta_reuse() {
    let mut funcalls = FunCalls::new();

    // Act: create 3 items
    funcalls.delta_index(0).unwrap();
    funcalls.delta_index(1).unwrap();
    funcalls.delta_index(2).unwrap();

    // Assert: 3 items are created
    let tool_calls = funcalls.get_tool_calls();
    assert_eq!(
        tool_calls,
        &vec![
            ContentItemFunction::default(),
            ContentItemFunction::default(),
            ContentItemFunction::default(),
        ]
    );

    // Act: create a new item
    funcalls.delta_index(3).unwrap();

    // Assert: now there are 4 items
    let tool_calls = funcalls.get_tool_calls();
    assert_eq!(
        tool_calls,
        &vec![
            ContentItemFunction::default(),
            ContentItemFunction::default(),
            ContentItemFunction::default(),
            ContentItemFunction::default(),
        ]
    );
}

#[test]
fn has_cell_for_delta() {
    let mut funcalls = FunCalls::new();
    let expected_err = "No active delta index";

    let result = funcalls.delta_id("foo");
    assert_eq!(result.unwrap_err(), expected_err);

    let result = funcalls.delta_function_name("foo");
    assert_eq!(result.unwrap_err(), expected_err);

    let result = funcalls.delta_function_arguments("foo");
    assert_eq!(result.unwrap_err(), expected_err);
}

#[test]
fn delta_index_regress_scenario() {
    let mut funcalls = FunCalls::new();

    // Simulate the delta_index_regress.txt scenario:
    // First tool call with index 0
    funcalls.delta_index(0).unwrap();
    funcalls.delta_id("call_O8vJyvRJrH6ST1ssD97c3jPI").unwrap();
    funcalls.delta_function_name("get_user_name").unwrap();
    funcalls.delta_function_arguments("{}").unwrap();

    // Second tool call with index 1
    funcalls.delta_index(1).unwrap();
    funcalls.delta_id("call_5fx8xXsKGpAhCNDTZsYoWWUx").unwrap();
    funcalls.delta_function_name("get_user_name").unwrap();
    funcalls.delta_function_arguments("{}").unwrap();

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
