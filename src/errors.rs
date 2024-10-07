use markdown::mdast::Node;

use crate::{
    document::{AdjustedPoint, Location},
    rules::RuleContext,
};

#[derive(Debug)]
pub struct LintError {
    pub message: String,
    pub location: Location,
    pub fix: Option<Vec<LintFix>>,
}

#[derive(Debug)]
pub enum LintFix {
    Insert(LintFixInsert),
    Delete(LintFixDelete),
    Replace(LintFixReplace),
}

#[derive(Debug)]
pub struct LintFixInsert {
    /// Text is inserted in front of this point
    pub point: AdjustedPoint,
    pub text: String,
}

#[derive(Debug)]
pub struct LintFixDelete {
    pub location: Location,
}

#[derive(Debug)]
pub struct LintFixReplace {
    pub location: Location,
}

impl LintError {
    pub fn new(message: String, location: Location, fix: Option<Vec<LintFix>>) -> Self {
        Self {
            message,
            location,
            fix,
        }
    }

    pub fn from_node(node: &Node, context: &RuleContext, message: &str) -> Option<Self> {
        if let Some(position) = node.position() {
            let location = Location::from_position(position, context);
            Some(Self::new(message.into(), location, None))
        } else {
            None
        }
    }

    pub fn from_node_with_fix(
        node: &Node,
        context: &RuleContext,
        message: &str,
        fix: Vec<LintFix>,
    ) -> Option<Self> {
        let mut lint_error = Self::from_node(node, context, message)?;
        lint_error.fix = Some(fix);
        Some(lint_error)
    }
}
