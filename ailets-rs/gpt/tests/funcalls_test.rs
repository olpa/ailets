use gpt::funcalls::{FunCalls, FunCallsWrite};

/// Test implementation of FunCallsWrite that stores calls for verification
#[derive(Debug, Default)]
struct TestFunCallsWrite {
    items: Vec<(String, String, String)>, // (id, name, arguments)
    current_arguments: String,
}

impl TestFunCallsWrite {
    fn new() -> Self {
        Self {
            items: Vec::new(),
            current_arguments: String::new(),
        }
    }

    fn get_items(&self) -> &Vec<(String, String, String)> {
        &self.items
    }
}

impl FunCallsWrite for TestFunCallsWrite {
    fn new_item(
        &mut self,
        _index: usize,
        id: String,
        name: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Store the id and name, reset arguments accumulator
        self.current_arguments.clear();
        // We'll store the complete item in end_item()
        self.items.push((id, name, String::new()));
        Ok(())
    }

    fn arguments_chunk(&mut self, ac: String) -> Result<(), Box<dyn std::error::Error>> {
        // Accumulate arguments chunks
        self.current_arguments.push_str(&ac);
        Ok(())
    }

    fn end_item(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Update the last item with the accumulated arguments
        if let Some(last) = self.items.last_mut() {
            last.2 = self.current_arguments.clone();
        }
        Ok(())
    }

    fn end(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // No special handling needed for this test implementation
        Ok(())
    }
}

//
// "Happy path" style tests
//

// Terminology and differences:
// - "Direct" funcalls: without using "index", using "end_current" to finalize
// - "Streaming" funcalls: using "index" to indicate progress

#[test]
fn single_funcall_direct() {
    // Arrange
    let mut writer = TestFunCallsWrite::new();
    let mut funcalls = FunCalls::new();

    // Act
    // Don't call "index"
    funcalls
        .id("call_9cFpsOXfVWMUoDz1yyyP1QXD", &mut writer)
        .unwrap();
    funcalls.name("get_user_name", &mut writer).unwrap();
    funcalls.arguments_chunk("{}", &mut writer).unwrap();
    funcalls.end_current(&mut writer).unwrap();

    // Assert
    funcalls.end(&mut writer).unwrap();
    let items = writer.get_items();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0],
        (
            "call_9cFpsOXfVWMUoDz1yyyP1QXD".to_string(),
            "get_user_name".to_string(),
            "{}".to_string()
        )
    );
}

#[test]
fn several_funcalls_direct() {
    // Arrange
    let mut writer = TestFunCallsWrite::new();
    let mut funcalls = FunCalls::new();

    // First tool call - Don't call "index"
    funcalls.id("call_foo", &mut writer).unwrap();
    funcalls.name("get_foo", &mut writer).unwrap();
    funcalls.arguments_chunk("{foo_args}", &mut writer).unwrap();
    funcalls.end_current(&mut writer).unwrap();

    // Second tool call - Don't call "index"
    funcalls.id("call_bar", &mut writer).unwrap();
    funcalls.name("get_bar", &mut writer).unwrap();
    funcalls.arguments_chunk("{bar_args}", &mut writer).unwrap();
    funcalls.end_current(&mut writer).unwrap();

    // Third tool call - Don't call "index"
    funcalls.id("call_baz", &mut writer).unwrap();
    funcalls.name("get_baz", &mut writer).unwrap();
    funcalls.arguments_chunk("{baz_args}", &mut writer).unwrap();
    funcalls.end_current(&mut writer).unwrap();

    // Assert
    funcalls.end(&mut writer).unwrap();
    let items = writer.get_items();
    assert_eq!(items.len(), 3);
    assert_eq!(
        items[0],
        (
            "call_foo".to_string(),
            "get_foo".to_string(),
            "{foo_args}".to_string()
        )
    );
    assert_eq!(
        items[1],
        (
            "call_bar".to_string(),
            "get_bar".to_string(),
            "{bar_args}".to_string()
        )
    );
    assert_eq!(
        items[2],
        (
            "call_baz".to_string(),
            "get_baz".to_string(),
            "{baz_args}".to_string()
        )
    );
}

#[test]
fn single_element_streaming() {
    // Arrange
    let mut writer = TestFunCallsWrite::new();
    let mut funcalls = FunCalls::new();

    // Act - streaming mode with delta_index
    funcalls.index(0, &mut writer).unwrap();

    funcalls
        .id("call_9cFpsOXfVWMUoDz1yyyP1QXD", &mut writer)
        .unwrap();
    funcalls.name("get_user_name", &mut writer).unwrap();
    funcalls.arguments_chunk("{}", &mut writer).unwrap();
    funcalls.end_current(&mut writer).unwrap();

    // Assert
    funcalls.end(&mut writer).unwrap();
    let items = writer.get_items();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0],
        (
            "call_9cFpsOXfVWMUoDz1yyyP1QXD".to_string(),
            "get_user_name".to_string(),
            "{}".to_string()
        )
    );
}

#[test]
fn several_elements_streaming() {
    // Arrange
    let mut writer = TestFunCallsWrite::new();
    let mut funcalls = FunCalls::new();

    // Act - streaming mode with delta_index, multiple elements in one round
    funcalls.index(0, &mut writer).unwrap();

    funcalls.id("call_foo", &mut writer).unwrap();
    funcalls.name("get_foo", &mut writer).unwrap();
    funcalls.arguments_chunk("{foo_args}", &mut writer).unwrap();

    funcalls.index(1, &mut writer).unwrap();

    funcalls.id("call_bar", &mut writer).unwrap();
    funcalls.name("get_bar", &mut writer).unwrap();
    funcalls.arguments_chunk("{bar_args}", &mut writer).unwrap();

    funcalls.index(2, &mut writer).unwrap();

    funcalls.id("call_baz", &mut writer).unwrap();
    funcalls.name("get_baz", &mut writer).unwrap();
    funcalls.arguments_chunk("{baz_args}", &mut writer).unwrap();

    // Assert
    funcalls.end(&mut writer).unwrap();
    let items = writer.get_items();
    assert_eq!(items.len(), 3);
    assert_eq!(
        items[0],
        (
            "call_foo".to_string(),
            "get_foo".to_string(),
            "{foo_args}".to_string()
        )
    );
    assert_eq!(
        items[1],
        (
            "call_bar".to_string(),
            "get_bar".to_string(),
            "{bar_args}".to_string()
        )
    );
    assert_eq!(
        items[2],
        (
            "call_baz".to_string(),
            "get_baz".to_string(),
            "{baz_args}".to_string()
        )
    );
}

//
// More detailed tests
//

#[test]
fn index_increment_validation() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // First index must be 0
    assert!(funcalls.index(0, &mut writer).is_ok());

    // Index can stay the same
    assert!(funcalls.index(0, &mut writer).is_ok());

    // Index can increment by 1
    assert!(funcalls.index(1, &mut writer).is_ok());

    // Index can stay the same
    assert!(funcalls.index(1, &mut writer).is_ok());

    // Index can increment by 1
    assert!(funcalls.index(2, &mut writer).is_ok());

    // Index cannot skip
    let result = funcalls.index(4, &mut writer);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("cannot skip values"));

    // Index cannot go backwards (never decreases)
    let result = funcalls.index(1, &mut writer);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("cannot decrease"));
}

#[test]
fn first_index_must_be_zero() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // First index must be 0
    let result = funcalls.index(1, &mut writer);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("First tool call index must be 0"));
}

#[test]
fn arguments_span_multiple_deltas() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // Enable streaming mode
    assert!(funcalls.index(0, &mut writer).is_ok());

    // Set up id and name first so new_item gets called
    funcalls.id("call_123", &mut writer).unwrap();
    funcalls.name("get_user", &mut writer).unwrap();

    // Arguments can be set multiple times - this should work
    funcalls.arguments_chunk("{", &mut writer).unwrap();
    funcalls
        .arguments_chunk("\"arg\": \"value\"", &mut writer)
        .unwrap();
    funcalls.arguments_chunk("}", &mut writer).unwrap();

    // End the item
    funcalls.end_item(&mut writer).unwrap();

    // No error should occur - arguments are allowed to span deltas
    let items = writer.get_items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].2, "{\"arg\": \"value\"}");
}

#[test]
fn test_id_already_given_error() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // First ID should work
    funcalls.id("call_123", &mut writer).unwrap();

    // Second ID should error
    let result = funcalls.id("call_456", &mut writer);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID is already given"));
}

#[test]
fn test_name_already_given_error() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // First name should work
    funcalls.name("get_user", &mut writer).unwrap();

    // Second name should error
    let result = funcalls.name("get_data", &mut writer);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Name is already given"));
}

#[test]
fn test_id_then_name_calls_new_item() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // Set id first, then name
    funcalls.id("call_123", &mut writer).unwrap();
    funcalls.name("get_user", &mut writer).unwrap();

    // Should have called new_item with both values
    let items = writer.get_items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0, "call_123");
    assert_eq!(items[0].1, "get_user");
}

#[test]
fn test_name_then_id_calls_new_item() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // Set name first, then id
    funcalls.name("get_user", &mut writer).unwrap();
    funcalls.id("call_123", &mut writer).unwrap();

    // Should have called new_item with both values
    let items = writer.get_items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].0, "call_123");
    assert_eq!(items[0].1, "get_user");
}

#[test]
fn test_arguments_chunk_without_new_item_stores() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // Add arguments without calling new_item first
    funcalls
        .arguments_chunk("{\"arg\": \"value\"}", &mut writer)
        .unwrap();

    // Should not have called writer.arguments_chunk yet
    // Now set id and name to trigger new_item
    funcalls.id("call_123", &mut writer).unwrap();
    funcalls.name("get_user", &mut writer).unwrap();

    // Now end the item
    funcalls.end_item(&mut writer).unwrap();

    let items = writer.get_items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].2, "{\"arg\": \"value\"}");
}

#[test]
fn test_arguments_chunk_with_new_item_forwards() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // Set id and name to trigger new_item
    funcalls.id("call_123", &mut writer).unwrap();
    funcalls.name("get_user", &mut writer).unwrap();

    // Now add arguments - should forward directly to writer
    funcalls
        .arguments_chunk("{\"arg\": \"value\"}", &mut writer)
        .unwrap();
    funcalls.end_item(&mut writer).unwrap();

    let items = writer.get_items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].2, "{\"arg\": \"value\"}");
}

#[test]
fn test_end_item_without_new_item_error() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // Call end_item without new_item should error
    let result = funcalls.end_item(&mut writer);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("end_item called without new_item being called first"));
}

#[test]
fn test_index_increment_calls_end_item_if_not_called() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // Start with index 0
    funcalls.index(0, &mut writer).unwrap();
    funcalls.id("call_123", &mut writer).unwrap();
    funcalls.name("get_user", &mut writer).unwrap();
    funcalls.arguments_chunk("{}", &mut writer).unwrap();

    // Move to index 1 without calling end_item - should auto-call it
    funcalls.index(1, &mut writer).unwrap();

    // The first item should be completed
    let items = writer.get_items();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0],
        (
            "call_123".to_string(),
            "get_user".to_string(),
            "{}".to_string()
        )
    );
}

#[test]
fn test_end_calls_end_item_if_not_called() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // Set up a function call
    funcalls.id("call_123", &mut writer).unwrap();
    funcalls.name("get_user", &mut writer).unwrap();
    funcalls.arguments_chunk("{}", &mut writer).unwrap();

    // Call end without calling end_item first
    funcalls.end(&mut writer).unwrap();

    // Should have auto-called end_item
    let items = writer.get_items();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0],
        (
            "call_123".to_string(),
            "get_user".to_string(),
            "{}".to_string()
        )
    );
}

#[test]
fn test_multiple_arguments_chunks_accumulated() {
    let mut funcalls = FunCalls::new();
    let mut writer = TestFunCallsWrite::new();

    // Add multiple argument chunks before new_item
    funcalls.arguments_chunk("{", &mut writer).unwrap();
    funcalls.arguments_chunk("\"key\":", &mut writer).unwrap();
    funcalls.arguments_chunk("\"value\"", &mut writer).unwrap();
    funcalls.arguments_chunk("}", &mut writer).unwrap();

    // Set id and name to trigger new_item
    funcalls.id("call_123", &mut writer).unwrap();
    funcalls.name("get_user", &mut writer).unwrap();
    funcalls.end_item(&mut writer).unwrap();

    let items = writer.get_items();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].2, "{\"key\":\"value\"}");
}
