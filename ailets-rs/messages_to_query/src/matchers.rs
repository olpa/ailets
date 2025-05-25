use scan_json::{ContextFrame, Matcher};

#[derive(Debug)]
pub struct ParentParentParentAndName {
    ancestor: String,
    grandparent: String,
    parent: String,
    name: String,
}

impl ParentParentParentAndName {
    #[must_use]
    pub fn new(ancestor: String, grandparent: String, parent: String, name: String) -> Self {
        Self {
            ancestor,
            grandparent,
            parent,
            name,
        }
    }
}

impl Matcher for ParentParentParentAndName {
    fn matches(&self, name: &str, context: &[ContextFrame]) -> bool {
        // Check name
        if self.name != name {
            return false;
        }

        let mut iter = context.iter().rev();

        // Check parent
        let Some(parent) = iter.next() else {
            return false;
        };
        if self.parent != parent.current_key {
            return false;
        }

        // Check grandparent
        let Some(grandparent) = iter.next() else {
            return false;
        };
        if self.grandparent != grandparent.current_key {
            return false;
        }

        // Check ancestor
        let Some(ancestor) = iter.next() else {
            return false;
        };
        self.ancestor == ancestor.current_key
    }
}
