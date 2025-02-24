#[derive(Debug, Default, PartialEq, Eq)]
pub struct ContentItemFunction {
    // type: "function",
    id: String,
    function_name: String,
    function_arguments: String,
}

impl ContentItemFunction {
    pub fn new(id: String, function_name: String, function_arguments: String) -> Self {
        Self {
            id,
            function_name,
            function_arguments,
        }
    }
}

pub trait FunCallsTrait {
    fn start_delta_round(&mut self);
    fn append(&mut self);
    fn delta_index(&mut self, index: usize);
    fn delta_id(&mut self, id: String);
    fn delta_function_name(&mut self, function_name: String);
    fn delta_function_arguments(&mut self, function_arguments: String);
    fn get_tool_calls(&self) -> &Vec<ContentItemFunction>;
}

pub struct FunCalls {
    idx: usize,
    tool_calls: Vec<ContentItemFunction>,
}

impl FunCalls {
    pub fn new() -> Self {
        Self {
            idx: 0,
            tool_calls: Vec::new(),
        }
    }
}

impl FunCallsTrait for FunCalls {

    fn start_delta_round(&mut self) {
        self.idx = 0;
    }

    fn append(&mut self) {
        self.tool_calls.push(ContentItemFunction::default());
    }

    fn delta_index(&mut self, index: usize) {
        self.idx = index;
    }

    fn delta_id(&mut self, id: String) {
        self.tool_calls[self.idx].id = id;
    }

    fn delta_function_name(&mut self, function_name: String) {
        self.tool_calls[self.idx].function_name = function_name;
    }

    fn delta_function_arguments(&mut self, function_arguments: String) {
        self.tool_calls[self.idx].function_arguments = function_arguments;
    }

    fn get_tool_calls(&self) -> &Vec<ContentItemFunction> {
        &self.tool_calls
    }
}
