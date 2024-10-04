use markdown::mdast::Node;

use crate::{document::Point, errors::LintError, parser::ParseResult};

mod rule001_heading_case;

pub trait Rule {
    fn setup(&self, context: &RuleContext) {}
    fn filter(&self, ast: &Node, context: &RuleContext) -> bool;
    fn check(&self, ast: &Node, context: &RuleContext) -> Vec<LintError>;
}

pub struct RuleContext {
    parse_result: ParseResult,
}

impl RuleContext {
    pub fn adjust_for_frontmatter_lines(&self, mut point: Point) -> Point {
        point.add_lines(self.parse_result.frontmatter_lines);
        point
    }
}
