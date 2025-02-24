use gpt::funcall::{ContentItemFunction, FunCalls};

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

#[test]
fn check_index() {
    // Arrange
    let mut funcalls = FunCalls::new();
    funcalls.start_delta_round();

    // Act
    funcalls.start_delta();
    assert!(funcalls.delta_index(0).is_ok());
    assert!(funcalls.delta_index(1).is_err());

    funcalls.start_delta();
    assert!(funcalls.delta_index(1).is_ok());
    assert!(funcalls.delta_index(0).is_err());
}

#[test]
fn delta_appends() {
    // Arrange
    let mut funcalls = FunCalls::new();
    funcalls.start_delta_round();

    // Act
    funcalls.start_delta();
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
