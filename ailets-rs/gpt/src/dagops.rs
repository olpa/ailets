//! DAG Operations Module

use crate::funcalls::FunCalls;

pub trait InjectDagOpsTrait {
    /// # Errors
    /// Promotes errors from the host.
    fn inject_funcalls(&self, funcalls: &FunCalls) -> Result<(), String>;
}

pub struct InjectDagOps;

impl InjectDagOpsTrait for InjectDagOps {
    fn inject_funcalls(&self, _funcalls: &FunCalls) -> Result<(), String> {
        Ok(())
    }
}
