use crate::Peek;
use crate::RJiter;
use std::cell::RefCell;
use std::io;

#[derive(Debug)]
pub struct Matcher {
    name: String,
    ctx: Option<String>,
    ctx2: Option<String>,
    ctx3: Option<String>,
}

impl Matcher {
    pub fn new(
        name: String,
        ctx: Option<String>,
        ctx2: Option<String>,
        ctx3: Option<String>,
    ) -> Self {
        Self {
            name,
            ctx,
            ctx2,
            ctx3,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum ActionResult {
    Ok,
    OkValueIsConsumed,
}

// ----

type TriggerAction<T> = Box<dyn Fn(&RefCell<RJiter>, &RefCell<T>) -> ActionResult>;

pub struct Trigger<T> {
    pub matcher: Matcher,
    pub action: TriggerAction<T>,
}

impl<T> std::fmt::Debug for Trigger<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Trigger {{ matcher: {:?}, action: <fn> }}", self.matcher)
    }
}

impl<T> Trigger<T> {
    pub fn new(matcher: Matcher, action: TriggerAction<T>) -> Self {
        Self { matcher, action }
    }
}

type TriggerEndAction<T> = Box<dyn Fn(&RefCell<T>)>;

pub struct TriggerEnd<T> {
    pub matcher: Matcher,
    pub action: TriggerEndAction<T>,
}

impl<T> std::fmt::Debug for TriggerEnd<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TriggerEnd {{ matcher: {:?}, action: <fn> }}",
            self.matcher
        )
    }
}

impl<T> TriggerEnd<T> {
    pub fn new(matcher: Matcher, action: TriggerEndAction<T>) -> Self {
        Self { matcher, action }
    }
}

#[derive(Debug)]
struct Context {
    current_key: String,
    is_in_object: bool,
    is_in_array: bool,
}

trait HasMatcher<A> {
    fn get_action(&self) -> &A;
    fn get_matcher(&self) -> &Matcher;
}

fn find_trigger_action<'a, 'b, 'c, T, A>(
    triggers: &'a Vec<T>,
    for_key: &'b String,
    context: &'c Vec<Context>,
) -> Option<&'a A>
where
    T: HasMatcher<A>,
{
    triggers
        .iter()
        .find(|trigger| {
            let matcher = trigger.get_matcher();
            if matcher.name != *for_key {
                return false;
            }

            let ctx_depth = context.len();
            if let Some(ref ctx1) = matcher.ctx {
                if ctx_depth < 1 || context[ctx_depth - 1].current_key != *ctx1 {
                    return false;
                }
            }

            if let Some(ref ctx2) = matcher.ctx2 {
                if ctx_depth < 2 || context[ctx_depth - 2].current_key != *ctx2 {
                    return false;
                }
            }

            if let Some(ref ctx3) = matcher.ctx3 {
                if ctx_depth < 3 || context[ctx_depth - 3].current_key != *ctx3 {
                    return false;
                }
            }

            true
        })
        .map(|trigger| trigger.get_action())
}

impl<T> HasMatcher<TriggerAction<T>> for Trigger<T> {
    fn get_action(&self) -> &TriggerAction<T> {
        &self.action
    }
    
    fn get_matcher(&self) -> &Matcher {
        &self.matcher
    }
}

impl<T> HasMatcher<TriggerEndAction<T>> for TriggerEnd<T> {
    fn get_action(&self) -> &TriggerEndAction<T> {
        &self.action
    }
    
    fn get_matcher(&self) -> &Matcher {
        &self.matcher
    }
}

fn find_action<'a, 'b, 'c, T>(
    triggers: &'a Vec<Trigger<T>>,
    for_key: &'b String,
    context: &'c Vec<Context>,
) -> Option<&'a TriggerAction<T>> {
    find_trigger_action(triggers, for_key, context)
}

fn find_end_action<'a, 'b, 'c, T>(
    triggers: &'a Vec<TriggerEnd<T>>,
    for_key: &'b String,
    context: &'c Vec<Context>,
) -> Option<&'a TriggerEndAction<T>> {
    find_trigger_action(triggers, for_key, context)
}

// ----

#[allow(clippy::missing_panics_doc)]
pub fn scan_json<T>(
    triggers: &Vec<Trigger<T>>,
    triggers_end: &Vec<TriggerEnd<T>>,
    rjiter_cell: &RefCell<RJiter>,
    baton_cell: &RefCell<T>,
) {
    let mut context: Vec<Context> = Vec::new();
    let mut is_object_begin = false;
    let mut is_in_object = false;
    let mut is_in_array = false;
    //let mut peeked = Peek::None;
    let mut current_key: String = "#top".to_string();
    let mut peeked: Option<Peek>;
    loop {
        peeked = None;

        let mut action = None;
        if is_in_object {
            let mut rjiter = rjiter_cell.borrow_mut();

            let keyr = if is_object_begin {
                rjiter.next_object()
            } else {
                rjiter.next_key()
            };
            is_object_begin = false;
            let key = keyr.unwrap();
            if key.is_none() {
                let ctx = context.pop().unwrap();
                current_key = ctx.current_key;
                is_in_array = ctx.is_in_array;
                is_in_object = ctx.is_in_object;
                let end_action = find_end_action(triggers_end, &current_key, &context);
                if let Some(end_action) = end_action {
                    end_action(baton_cell);
                }
                continue; // continue
            }
            current_key = key.unwrap().to_string();

            action = find_action(triggers, &current_key, &context);
        }

        if let Some(action) = action {
            let result = action(rjiter_cell, baton_cell);
            if result == ActionResult::OkValueIsConsumed {
                continue;
            }
        }

        let mut rjiter = rjiter_cell.borrow_mut();

        if is_in_array {
            let apickedr = if is_object_begin {
                rjiter.known_array()
            } else {
                rjiter.array_step()
            };
            is_object_begin = false;
            peeked = apickedr.unwrap();
            if peeked.is_none() {
                let ctx = context.pop().unwrap();
                current_key = ctx.current_key;
                is_in_array = ctx.is_in_array;
                is_in_object = ctx.is_in_object;
                continue; // continue
            }
        }

        if peeked.is_none() {
            let peekedr = rjiter.peek();
            if let Err(jiter::JiterError {
                error_type:
                    jiter::JiterErrorType::JsonError(jiter::JsonErrorType::EofWhileParsingValue),
                ..
            }) = peekedr
            {
                assert!(context.is_empty(), "scan_json: eof while inside an object");
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
                is_in_object,
                is_in_array,
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
                is_in_object,
                is_in_array,
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
            rjiter.write_bytes(&mut io::sink()).unwrap();
            continue;
        }

        let maybe_number = rjiter.next_number();
        if maybe_number.is_ok() {
            continue;
        }
        panic!("scan_json: unhandled: peeked={peeked:?}");
    }
}
