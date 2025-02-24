#[derive(Debug, Default)]
struct ContentItemFunction {
    // type: "function",
    id: String,
    function_name: String,
    function_arguments: String,
}

trait FunCalls {
    fn start_delta_round(&mut self);
    fn append(&mut self);
    fn delta_id(&mut self, index: usize, id: String);
    fn delta_function_name(&mut self, index: usize, function_name: String);
    fn delta_function_arguments(&mut self, index: usize, function_arguments: String);
    fn get_tool_calls(&self) -> Vec<ContentItemFunction>;
}

struct FunCalls {
    idx: usize,
    tool_calls: Vec<ContentItemFunction>,
}

impl FunCalls for FunCalls {
    fn new() -> Self {
        Self {
            idx: 0,
            tool_calls: Vec::new(),
        }
    }

    fn start_delta_round(&mut self) {
        self.idx = 0;
    }

    fn append(&mut self) {
        self.tool_calls.push(ContentItemFunction::default());
    }

    fn delta_id(&mut self, index: usize, id: String) {
        self.tool_calls[index].id = id;
    }

    fn delta_function_name(&mut self, index: usize, function_name: String) {
        self.tool_calls[index].function_name = function_name;
    }

    fn delta_function_arguments(&mut self, index: usize, function_arguments: String) {
        self.tool_calls[index].function_arguments = function_arguments;
    }

    fn get_tool_calls(&self) -> Vec<ContentItemFunction> {
        self.tool_calls.clone()
    }
}
