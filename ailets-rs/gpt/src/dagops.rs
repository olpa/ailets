use crate::funcall::FunCalls;

#[allow(clippy::missing_errors_doc)]
pub trait DagOpsTrait {
    fn inject_funcalls(&self, funcalls: &FunCalls) -> Result<(), String>;
}

pub struct DummyDagOps;

impl DummyDagOps {
    #[allow(clippy::new_without_default)]
    #[must_use]
    pub fn new() -> Self {
        Self {}
    }
}

impl DagOpsTrait for DummyDagOps {
    fn inject_funcalls(&self, _funcalls: &FunCalls) -> Result<(), String> {
        Ok(())
    }
}
