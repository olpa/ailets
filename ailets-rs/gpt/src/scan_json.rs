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
    let mut peeked = rjiter.peek().unwrap(); // FIXME
    loop {
        if is_in_object {
            let peekedr = rjiter.peek();
            println!("in object: peeked={peekedr:?}, exit");
            break;
        }

        let peekedr = rjiter.peek();
        if let Err(jiter::JiterError {
            error_type: jiter::JiterErrorType::JsonError(jiter::JsonErrorType::EofWhileParsingValue),
            ..
        }) = peekedr
        {
            // TODO: check that we are on top, not inside object or array
            break;
        }

        peeked = peekedr.unwrap();

        if peeked == Peek::Null {
            rjiter.known_null().unwrap();
            continue;
        }
        if peeked == Peek::True {
            rjiter.known_bool(peeked).unwrap();
            continue;
        }
        if peeked == Peek::False {
            rjiter.known_bool(peeked).unwrap();
            continue;
        }
        if peeked == Peek::String {
            rjiter.write_bytes(None).unwrap();
            continue;
        }

        let maybe_number = rjiter.next_number();
        if let Ok(number) = maybe_number {
            continue;
        }
        panic!("scan_json: unhandled: peeked={peeked:?}");
    }
}
