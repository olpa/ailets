use std::cell::RefCell;

use gpt::dagops::InjectDagOpsTrait;
use gpt::funcalls::{ContentItemFunction, FunCalls};

pub struct TrackedInjectDagOps {
    funcalls: RefCell<FunCalls>,
}

impl TrackedInjectDagOps {
    #[allow(clippy::new_without_default)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            funcalls: RefCell::new(FunCalls::new()),
        }
    }

    pub fn get_funcalls(&self) -> Vec<ContentItemFunction> {
        self.funcalls.borrow().get_tool_calls().clone()
    }
}

impl InjectDagOpsTrait for TrackedInjectDagOps {
    fn inject_funcalls(&self, funcalls: &FunCalls) -> Result<(), String> {
        *self.funcalls.borrow_mut() = funcalls.clone();
        Ok(())
    }
}
