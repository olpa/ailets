pub struct SxsltMatcher<'a> {
    name: &'a str,
    ctx: Option<&'a str>,
    ctx2: Option<&'a str>,
    ctx3: Option<&'a str>,
}

impl<'a> SxsltMatcher<'a> {
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
