use std::cell::RefCell;
use crate::awriter::AWriter;
use crate::rjiter::RJiter;
use crate::scan_json::{ActionResult};

pub struct SSEHandler {
    awriter: RefCell<AWriter>,
}

impl SSEHandler {
    pub fn new(awriter: RefCell<AWriter>) -> Self {
        SSEHandler { awriter }
    }

    pub fn end(&mut self) {
        let mut awriter = self.awriter.borrow_mut();
        awriter.end_message();
    }
}

/*
pub fn on_end_delta(_rjiter: &RefCell<RJiter>, sh: &RefCell<SSEHandler>) -> ActionResult {
    rjiter.borrow_mut().feed();
    ActionResult::Ok
}
*/

pub fn on_delta_role<'rj>(
    rjiter: &'rj RefCell<RJiter<'rj>>,
    sh: &'rj RefCell<SSEHandler>
) -> ActionResult {
    let mut rjiter = rjiter.borrow_mut();
    let sh = sh.borrow();
    let awriter = &mut *sh.awriter.borrow_mut();

    let role = rjiter.next_str();
    assert!(role.is_ok(), "Error handling role: {role:?}");
    awriter.role(role.unwrap());

    ActionResult::OkValueIsConsumed
}

pub fn on_delta_content<'rj>(
    rjiter: &'rj RefCell<RJiter<'rj>>,
    sh: &'rj RefCell<SSEHandler>
) -> ActionResult {
    let mut rjiter = rjiter.borrow_mut();
    let sh = sh.borrow();
    let awriter = &mut *sh.awriter.borrow_mut();

    awriter.begin_text_content();
    let wb = rjiter.write_bytes(awriter);
    assert!(wb.is_ok(), "Error handling content: {wb:?}");
    awriter.end_text_content();

    ActionResult::OkValueIsConsumed
}
