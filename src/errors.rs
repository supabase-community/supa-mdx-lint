use markdown::mdast::Node;

use crate::{document::Point, rules::RuleContext};

#[derive(Debug)]
pub struct LintError {
    message: String,
    location: Point,
}

pub enum FixType {
    Insert(LintFixInsert),
    Delete(LintFixDelete),
    Replace(LintFixReplace),
}

pub struct LintFixInsert {
    location: Point,
    text: String,
}

pub struct LintFixDelete {
    start_location: Point,
    /// Exclusive
    end_location: Point,
}

pub struct LintFixReplace {
    start_location: Point,
    /// Exclusive
    end_location: Point,
    text: String,
}

impl LintError {
    pub fn new(message: String, point: Point) -> Self {
        Self {
            message,
            location: point,
        }
    }

    pub fn from_node(node: &Node, context: &RuleContext, message: &str) -> Option<Self> {
        if let Some(position) = node.position() {
            let point = Point::from(position);
            let point = context.adjust_for_frontmatter_lines(point);
            Some(Self::new(message.into(), point))
        } else {
            None
        }
    }
}
