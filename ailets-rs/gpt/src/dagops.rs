//! DAG Operations Module
//!
//! This module provides traits and implementations for managing function calls in a DAG-like structure.
//! It includes:
//! - [`DagOpsTrait`]: A trait defining the interface for injecting function calls
//! - [`DummyDagOps`]: A no-op implementation that does nothing with injected calls
//! - [`TrackedDagOps`]: An implementation that tracks and stores function calls for later retrieval
//!
//! The module is designed to support both testing scenarios (using `DummyDagOps`) and
//! actual function call tracking (using `TrackedDagOps`).

use crate::funcalls::{ContentItemFunction, FunCalls};
use std::cell::RefCell;

pub trait DagOpsTrait {
    /// # Errors
    /// If anything goes wrong.
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

pub struct TrackedDagOps {
    funcalls: RefCell<FunCalls>,
}

impl TrackedDagOps {
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

impl DagOpsTrait for TrackedDagOps {
    fn inject_funcalls(&self, funcalls: &FunCalls) -> Result<(), String> {
        *self.funcalls.borrow_mut() = funcalls.clone();
        Ok(())
    }
}
