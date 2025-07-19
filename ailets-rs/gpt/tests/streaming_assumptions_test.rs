use gpt::funcalls::FunCalls;

#[test]
fn test_index_increment_validation() {
    let mut funcalls = FunCalls::new();
    
    // First index must be 0
    assert!(funcalls.delta_index(0).is_ok());
    
    // Index can stay the same
    assert!(funcalls.delta_index(0).is_ok());
    
    // Index can increment by 1
    assert!(funcalls.delta_index(1).is_ok());
    
    // Index can stay the same
    assert!(funcalls.delta_index(1).is_ok());
    
    // Index can increment by 1
    assert!(funcalls.delta_index(2).is_ok());
    
    // Index cannot skip
    let result = funcalls.delta_index(4);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("cannot skip values"));
    
    // Index cannot go backwards (never decreases)
    let result = funcalls.delta_index(1);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("cannot decrease"));
}

#[test]
fn test_id_field_assumption_violation() {
    let mut funcalls = FunCalls::new();
    
    // Enable streaming mode
    assert!(funcalls.delta_index(0).is_ok());
    
    // First ID set should work
    assert!(funcalls.delta_id("call_123").is_ok());
    
    // Second ID set in same streaming session should fail
    let result = funcalls.delta_id("_extra");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("ID field cannot be set multiple times"));
}

#[test]
fn test_name_field_assumption_violation() {
    let mut funcalls = FunCalls::new();
    
    // Enable streaming mode
    assert!(funcalls.delta_index(0).is_ok());
    
    // First name set should work
    assert!(funcalls.delta_function_name("get_user").is_ok());
    
    // Second name set in same streaming session should fail  
    let result = funcalls.delta_function_name("_plus");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Function name field cannot be set multiple times"));
}

#[test]
fn test_arguments_can_span_multiple_deltas() {
    let mut funcalls = FunCalls::new();
    
    // Enable streaming mode
    assert!(funcalls.delta_index(0).is_ok());
    
    // Arguments can be set multiple times - this should work
    funcalls.delta_function_arguments("{");
    funcalls.delta_function_arguments("\"arg\": \"value\"");
    funcalls.delta_function_arguments("}");
    
    // No error should occur - arguments are allowed to span deltas
}

#[test]
fn test_first_index_must_be_zero() {
    let mut funcalls = FunCalls::new();
    
    // First index must be 0
    let result = funcalls.delta_index(1);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("First tool call index must be 0"));
}