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

pub fn scan_json<T>(triggers: &[Trigger<T>], rjiter: &mut RJiter, _baton: T) {
    println!("scan_json: triggers={triggers:?}, rjiter={rjiter:?}");
}
