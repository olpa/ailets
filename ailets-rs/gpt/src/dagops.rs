use gpt::funcall::FunCalls;

pub trait DagOpsTrait {
    pub fn inject_funcalls(&mut self, funcalls: &FunCalls) -> Result<(), String>;
}
