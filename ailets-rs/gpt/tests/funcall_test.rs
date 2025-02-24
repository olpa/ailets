use crate::funcall::{ContentItemFunction, FunCalls};

#[test]
fn single_funcall() {
    let mut funcalls = FunCalls::new();
    
    // Start first round
    funcalls.start_delta_round();
    
    // Add one function call
    funcalls.append();
    
    // Set the function call details
    funcalls.delta_id(0, "call_9cFpsOXfVWMUoDz1yyyP1QXD".to_string());
    funcalls.delta_function_name(0, "get_user_name".to_string());
    funcalls.delta_function_arguments(0, "{}".to_string());

    // Get and verify results
    let tool_calls = funcalls.get_tool_calls();
    assert_eq!(tool_calls.len(), 1);
    
    let call = &tool_calls[0];
    assert_eq!(call, ContentItemFunction {
        id: "call_9cFpsOXfVWMUoDz1yyyP1QXD".to_string(),
        function_name: "get_user_name".to_string(),
        function_arguments: "{}".to_string(),
    });
}
