use std::cell::RefCell;

use crate::awriter::AWriter;
use crate::rjiter::RJiter;
use crate::scan_json::{ActionResult};

pub struct SSEHandler<'rjiter, 'awriter> {
    awriter: &'awriter RefCell<AWriter>,
    role: Option<&'rjiter str>,
    content: Option<&'rjiter str>,
}

impl<'rjiter, 'awriter> SSEHandler<'rjiter, 'awriter> {
    pub fn new(awriter: &'awriter RefCell<AWriter>) -> Self {
        SSEHandler { awriter, role: None, content: None }
    }

    pub fn end(&mut self) {
        let mut awriter = self.awriter.borrow_mut();
        awriter.end_message();
    }
}

pub fn on_begin_delta(_rjiter: &RefCell<RJiter>, sh: &RefCell<SSEHandler>) -> ActionResult {
    let mut sh = sh.borrow_mut();
    sh.role = None;
    sh.content = None;
    ActionResult::Ok
}

pub fn on_end_delta(rjiter: &RefCell<RJiter>, sh: &RefCell<SSEHandler>) -> ActionResult {
    let mut sh = sh.borrow_mut();
    if sh.role.is_some() {
        let mut awriter = sh.awriter.borrow_mut();
        awriter.begin_message();
        awriter.role(sh.role.unwrap());
    }
    if sh.content.is_some() {
        let mut awriter = sh.awriter.borrow_mut();
        awriter.begin_text_content();
        awriter.str(sh.content.unwrap());
        awriter.end_text_content();
    }

    sh.role = None;
    sh.content = None;
    rjiter.borrow_mut().feed();
    ActionResult::Ok
}

pub fn on_delta_role(rjiter: &RefCell<RJiter>, sh: &RefCell<SSEHandler>) -> ActionResult {
    let mut rjiter = rjiter.borrow_mut();
    let role = rjiter.next_str().unwrap();
    let mut sh = sh.borrow_mut();
    sh.role = Some(role);
    ActionResult::OkValueIsConsumed
}

pub fn on_delta_content(rjiter: &RefCell<RJiter>, sh: &RefCell<SSEHandler>) -> ActionResult {
    let mut rjiter = rjiter.borrow_mut();
    let content = rjiter.next_str().unwrap();
    let mut sh = sh.borrow_mut();
    sh.content = Some(content);
    ActionResult::OkValueIsConsumed
}