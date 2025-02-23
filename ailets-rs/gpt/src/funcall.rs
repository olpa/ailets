#[derive(Debug, Default)]
struct ContentItemFunction {
    // type: "function",
    id: String,
    function_name: String,
    function_arguments: String,
}

trait FunCalls {
    fn append(&mut self);
    fn set_id(&mut self, index: usize, id: String);
    fn set_function_name(&mut self, index: usize, function_name: String);
    fn set_function_arguments(&mut self, index: usize, function_arguments: String);
    fn get_tool_calls(&self) -> Vec<ContentItemFunction>;
}

struct FunCalls {
    tool_calls: Vec<ContentItemFunction>,
}

impl FunCalls for FunCalls {
    fn append(&mut self) {
        self.tool_calls.push(ContentItemFunction::default());
    }
}
