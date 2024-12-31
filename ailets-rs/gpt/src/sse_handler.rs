pub struct SSEHandler<'rjiter> {
    writer: &RefCell<AWriter>,
    role: Option<&'rjiter str>,
    content: Option<&'rjiter str>,
}

impl<'rjiter> SSEHandler<'rjiter> {
    pub fn new(writer: &RefCell<AWriter>) -> Self {
        SSEHandler { writer, role: None, content: None }
    }

    pub fn end(&mut self) {
        let writer = self.writer.borrow_mut();
        writer.end_message();
    }
}

pub fn on_begin_delta(_rjiter: &RefCell<Rjiter>, sh: &RefCell<SSEHandler<'rjiter>>) -> ActionResult {
    let mut sh = sh.borrow_mut();
    sh.role = None;
    sh.content = None;
    ActionResult::Ok
}

pub fn on_end_delta(_rjiter: &RefCell<Rjiter>, sh: &RefCell<SSEHandler<'rjiter>>) -> ActionResult {
    let mut sh = sh.borrow_mut();
    if sh.role.is_some() {
        let writer = sh.writer.borrow_mut();
        writer.begin_message();
        writer.role(sh.role.unwrap());
    }
    if sh.content.is_some() {
        let writer = sh.writer.borrow_mut();
        writer.begin_text_content();
        writer.str(sh.content.unwrap());
        writer.end_text_content();
    }
    ActionResult::Ok
}

pub fn on_delta_role(_rjiter: &RefCell<Rjiter>, sh: &RefCell<SSEHandler<'rjiter>>, role: &'rjiter str) -> ActionResult {
    let rjiter = rjiter.borrow();
    let role = rjiter.next_str(role).unwrap();
    let mut sh = sh.borrow_mut();
    sh.role = Some(role);
    ActionResult::OkValueIsConsumed
}

pub fn on_delta_content(_rjiter: &RefCell<Rjiter>, sh: &RefCell<SSEHandler<'rjiter>>, content: &'rjiter str) -> ActionResult {
    let rjiter = rjiter.borrow();
    let content = rjiter.next_str(content).unwrap();
    let mut sh = sh.borrow_mut();
    sh.content = Some(content);
    ActionResult::OkValueIsConsumed
}
