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

#[derive(Debug)]
struct Context {
    current_key: String,
    is_in_object: bool,
    is_in_array: bool,
}

#[allow(clippy::missing_panics_doc)]
pub fn scan_json<T>(triggers: &[Trigger<T>], rjiter: &mut RJiter, _baton: T) {
    let mut context: Vec<Context> = Vec::new();
    let mut is_object_begin = false;
    let mut is_in_object = false;
    let mut is_in_array = false;
    //let mut peeked = Peek::None;
    let mut current_key: String = "#top".to_string();
    let mut peeked: Option<Peek>;
    loop {
        peeked = None;

        if is_in_object {
            let keyr = if is_object_begin {
                rjiter.next_object()
            } else {
                rjiter.next_key()
            };
            is_object_begin = false;
            let key = keyr.unwrap();
            if key == None {
                let ctx = context.pop().unwrap();
                current_key = ctx.current_key;
                is_in_array = ctx.is_in_array;
                is_in_object = ctx.is_in_object;
                continue;                                   // continue
            }
            current_key = key.unwrap().to_string();
            // pass-through to consume the key value
        }

        if is_in_array {
            let apickedr = if is_object_begin {
                rjiter.known_array()
            } else {
                rjiter.array_step()
            };
            is_object_begin = false;
            peeked = apickedr.unwrap();
            if peeked == None {
                let ctx = context.pop().unwrap();
                current_key = ctx.current_key;
                is_in_array = ctx.is_in_array;
                is_in_object = ctx.is_in_object;
                continue;                                    // continue
            }
        }

        if peeked == None {
            let peekedr = rjiter.peek();
            if let Err(jiter::JiterError {
                error_type: jiter::JiterErrorType::JsonError(jiter::JsonErrorType::EofWhileParsingValue),
                ..
            }) = peekedr
            {
                if context.len() > 0 {
                    panic!("scan_json: eof while parsing value");
                }
                let eof = rjiter.finish();
                eof.unwrap();
                break;
            }

            peeked = Some(peekedr.unwrap());
        };

        let peeked = peeked.unwrap();

        if peeked == Peek::Array {
            context.push(Context {
                current_key: current_key.clone(),
                is_in_object: is_in_object,
                is_in_array: is_in_array,
            });
            current_key = "#array".to_string();
            is_in_array = true;
            is_in_object = false;
            is_object_begin = true;
            continue;
        }

        if peeked == Peek::Object {
            context.push(Context {
                current_key: current_key.clone(),
                is_in_object: is_in_object,
                is_in_array: is_in_array,
            });
            is_in_array = false;
            is_in_object = true;
            is_object_begin = true;
            continue;
        }

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
        if let Ok(_) = maybe_number {
            continue;
        }
        panic!("scan_json: unhandled: peeked={peeked:?}");
    }
}
