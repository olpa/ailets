//! DAG Operations Module

use crate::funcalls::{ContentItemFunction, FunCalls};
use std::cell::RefCell;

pub trait InjectDagOpsTrait {
    /// # Errors
    /// Promotes errors from the host.
    fn inject_funcalls(&self, funcalls: &FunCalls) -> Result<(), String>;
}

struct InjectDagOps;

impl InjectDagOpsTrait for InjectDagOps {
    fn inject_funcalls(&self, _funcalls: &FunCalls) -> Result<(), String> {
        Ok(())
    }
}
