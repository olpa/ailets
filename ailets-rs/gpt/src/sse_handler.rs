use crate::awriter::AWriter;
use crate::rjiter::RJiter;
use crate::scan_json::ActionResult;
use std::cell::RefCell;

pub struct SSEHandler<'awriter> {
    awriter: &'awriter mut AWriter,
}

impl<'awriter> SSEHandler<'awriter> {
    pub fn new(awriter: &'awriter mut AWriter) -> Self {
        SSEHandler { awriter }
    }

    pub fn end(&mut self) {
        self.awriter.end_message();
    }
}

#[allow(clippy::missing_panics_doc)]
pub fn on_delta_role<'rj>(
    rjiter: &'rj RefCell<RJiter<'rj>>,
    sh: &'rj RefCell<SSEHandler>,
) -> ActionResult {
    let mut rjiter = rjiter.borrow_mut();
    let mut sh = sh.borrow_mut();

    let role = rjiter.next_str();
    assert!(role.is_ok(), "Error handling role: {role:?}");
    sh.awriter.role(role.unwrap());

    ActionResult::OkValueIsConsumed
}

#[allow(clippy::missing_panics_doc)]
pub fn on_delta_content<'rj>(
    rjiter: &'rj RefCell<RJiter<'rj>>,
    sh: &'rj RefCell<SSEHandler>,
) -> ActionResult {
    let mut rjiter = rjiter.borrow_mut();
    let mut sh = sh.borrow_mut();

    sh.awriter.begin_text_chunk();
    let wb = rjiter.write_bytes(sh.awriter);
    assert!(wb.is_ok(), "Error handling content: {wb:?}");

    ActionResult::OkValueIsConsumed
}
