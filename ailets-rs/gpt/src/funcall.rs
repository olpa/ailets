#[derive(Debug, Default, PartialEq, Eq)]
pub struct ContentItemFunction {
    // type: "function",
    id: String,
    function_name: String,
    function_arguments: String,
}

impl ContentItemFunction {
    #[must_use]
    pub fn new(id: &str, function_name: &str, function_arguments: &str) -> Self {
        Self {
            id: id.to_string(),
            function_name: function_name.to_string(),
            function_arguments: function_arguments.to_string(),
        }
    }
}

pub trait FunCallsTrait {
    fn start_delta_round(&mut self);
    fn start_delta(&mut self);
    #[allow(clippy::missing_errors_doc)]
    fn delta_index(&mut self, index: usize) -> Result<(), String>;
    #[allow(clippy::missing_errors_doc)]
    fn delta_id(&mut self, id: &str) -> Result<(), String>;
    #[allow(clippy::missing_errors_doc)]
    fn delta_function_name(&mut self, function_name: &str) -> Result<(), String>;
    #[allow(clippy::missing_errors_doc)]
    fn delta_function_arguments(&mut self, function_arguments: &str) -> Result<(), String>;
    fn get_tool_calls(&self) -> &Vec<ContentItemFunction>;
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FunCalls {
    idx: usize,
    tool_calls: Vec<ContentItemFunction>,
}

impl FunCalls {
    #[must_use]
    pub fn new() -> Self {
        Self {
            idx: 0,
            tool_calls: Vec::new(),
        }
    }

    fn get_cell(&mut self) -> Result<&mut ContentItemFunction, String> {
        let len = self.tool_calls.len();
        let cell = self.tool_calls.get_mut(self.idx);
        match cell {
            Some(cell) => Ok(cell),
            None => Err(format!(
                "Delta index is out of bounds: {}, n of deltas: {}",
                self.idx, len
            )),
        }
    }
}

impl FunCallsTrait for FunCalls {
    fn start_delta_round(&mut self) {
        self.idx = usize::MAX;
    }

    fn start_delta(&mut self) {
        if self.idx == usize::MAX || self.idx >= self.tool_calls.len() {
            self.tool_calls.push(ContentItemFunction::default());
        }
        if self.idx == usize::MAX {
            self.idx = 0;
        } else {
            self.idx += 1;
        }
    }

    fn delta_index(&mut self, index: usize) -> Result<(), String> {
        if self.idx == index {
            return Ok(());
        }
        Err(format!(
            "Delta index mismatch. Got: {}, expected: {}",
            index, self.idx
        ))
    }

    fn delta_id(&mut self, id: &str) -> Result<(), String> {
        let cell = self.get_cell()?;
        cell.id = id.to_string();
        Ok(())
    }

    fn delta_function_name(&mut self, function_name: &str) -> Result<(), String> {
        let cell = self.get_cell()?;
        cell.function_name = function_name.to_string();
        Ok(())
    }

    fn delta_function_arguments(&mut self, function_arguments: &str) -> Result<(), String> {
        let cell = self.get_cell()?;
        cell.function_arguments = function_arguments.to_string();
        Ok(())
    }

    fn get_tool_calls(&self) -> &Vec<ContentItemFunction> {
        &self.tool_calls
    }
}
