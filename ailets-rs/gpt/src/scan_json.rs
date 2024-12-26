use crate::Peek;
use crate::RJiter;

#[derive(Debug)]
pub struct Matcher<'a> {
    name: &'a str,
    ctx: Option<&'a str>,
    ctx2: Option<&'a str>,
    ctx3: Option<&'a str>,
}

impl<'a> Matcher<'a> {
    pub fn new(
        name: &'a str,
        ctx: Option<&'a str>,
        ctx2: Option<&'a str>,
        ctx3: Option<&'a str>,
    ) -> Self {
        Self {
            name,
            ctx,
            ctx2,
            ctx3,
        }
    }
}

type TriggerAction<'a, T> = Box<dyn FnMut(&mut RJiter, T) + 'a>;

pub struct Trigger<'a, T> {
    matcher: Matcher<'a>,
    action: TriggerAction<'a, T>,
}

impl<'a, T> std::fmt::Debug for Trigger<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Trigger {{ matcher: {:?}, action: <fn> }}", self.matcher)
    }
}

impl<'a, T> Trigger<'a, T> {
    pub fn new(matcher: Matcher<'a>, action: TriggerAction<'a, T>) -> Self {
        Self { matcher, action }
    }
}

#[allow(clippy::missing_panics_doc)]
pub fn scan_json<T>(triggers: &[Trigger<T>], rjiter: &mut RJiter, _baton: T) {
    println!("scan_json: triggers={triggers:?}");
    let mut context: Vec<String> = Vec::new();
    let mut at_object_begin = false;
    let mut is_in_object = false;
    //let mut peeked = Peek::None;
    let mut peeked = rjiter.peek(); // FIXME
    loop {
        if is_in_object {
            peeked = rjiter.peek();
            println!("in object: peeked={peeked:?}, exit");
            break;
        }

        peeked = rjiter.peek();
        if let Err(jiter::JiterError {
            error_type: jiter::JiterErrorType::JsonError(jiter::JsonErrorType::EofWhileParsingValue),
            ..
        }) = peeked
        {
            break;
        }

        panic!("scan_json: unhandled: peeked={peeked:?}");
    }
}
