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

pub struct Trigger<'a, T> {
    matcher: Matcher<'a>,
    action: Box<dyn FnMut(T) + 'a>,
}

impl<'a, T> std::fmt::Debug for Trigger<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Trigger {{ matcher: {:?}, action: <fn> }}", self.matcher)
    }
}

impl<'a, T> Trigger<'a, T> {
    pub fn new(matcher: Matcher<'a>, action: Box<dyn FnMut(T) + 'a>) -> Self {
        Self { matcher, action }
    }
}
