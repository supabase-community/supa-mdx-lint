use markdown::mdast::Node;
use once_cell::sync::Lazy;

use crate::{document::Point, errors::LintError, parser::ParseResult};

pub mod rule001_heading_case;

use rule001_heading_case::Rule001HeadingCase;

static ALL_RULES: Lazy<Vec<Box<dyn Rule>>> =
    Lazy::new(|| vec![Box::new(Rule001HeadingCase::default())]);

pub trait Rule: Send + Sync + RuleName {
    fn setup(&self, context: &RuleContext) {}
    fn filter(&self, ast: &Node, context: &RuleContext) -> bool;
    fn check(&self, ast: &Node, context: &RuleContext) -> Vec<LintError>;
}

pub trait RuleName {
    fn name(&self) -> &'static str;
}

#[derive(Clone, Debug)]
pub struct RuleSettings(toml::Value);

impl RuleSettings {
    pub fn new(table: toml::Table) -> Self {
        Self(toml::Value::Table(table))
    }
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

pub struct RuleRegistry {
    rules: Vec<&'static dyn Rule>,
}

impl RuleRegistry {
    pub fn new() -> Self {
        let rules = ALL_RULES
            .iter()
            .map(|rule| rule.as_ref() as &dyn Rule)
            .collect();
        Self { rules }
    }

    pub fn is_valid_rule(rule_name: &str) -> bool {
        ALL_RULES.iter().any(|rule| rule.name() == rule_name)
    }

    pub fn deactivate_rule(&mut self, rule_name: &str) {
        self.rules.retain(|rule| rule.name() != rule_name);
    }

    #[cfg(test)]
    pub fn is_rule_active(&self, rule_name: &str) -> bool {
        self.rules.iter().any(|rule| rule.name() == rule_name)
    }
}
