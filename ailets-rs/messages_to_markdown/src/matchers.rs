use scan_json::ContextFrame;
use scan_json::Matcher;

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
        if self.name != name {
            return false;
        }
        let mut iter = context.iter().rev();

        // Check parent
        if let Some(frame) = iter.next() {
            if frame.current_key != self.parent {
                return false;
            }
        } else {
            return false;
        }

        // Check grandparent
        if let Some(frame) = iter.next() {
            if frame.current_key != self.grandparent {
                return false;
            }
        } else {
            return false;
        }

        // Check ancestor
        if let Some(frame) = iter.next() {
            if frame.current_key != self.ancestor {
                return false;
            }
        } else {
            return false;
        }

        true
    }
}
