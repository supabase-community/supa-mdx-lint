use std::collections::HashSet;
use supa_mdx_macros::RuleName;

use markdown::mdast::Node;

use crate::{errors::LintError, utils::get_text_content};

use super::{Rule, RuleContext, RuleName};

#[derive(Debug, Default, RuleName)]
pub struct Rule001HeadingCase {
    may_uppercase: HashSet<String>,
    may_lowercase: HashSet<String>,
}

impl Rule for Rule001HeadingCase {
    fn setup(&self, context: &RuleContext) {}

    fn filter(&self, ast: &Node, context: &RuleContext) -> bool {
        matches!(ast, Node::Heading(_))
    }

    fn check(&self, ast: &Node, context: &RuleContext) -> Vec<LintError> {
        let mut errors = Vec::new();

        if let Node::Heading(_) = ast {
            let text_content = get_text_content(ast);
            let text_content: Vec<&str> = text_content.split_whitespace().collect();

            if let Some(first_word) = text_content.first() {
                let first_letter_lowercase = first_word
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_lowercase());
                if first_letter_lowercase && !self.may_lowercase.contains(*first_word) {
                    let lint_error = LintError::from_node(
                        ast,
                        context,
                        "First word of heading should be capitalized",
                    );
                    if let Some(lint_error) = lint_error {
                        errors.push(lint_error);
                    }
                }
            }

            text_content.iter().skip(1).for_each(|word| {
                let first_letter_uppercase =
                    word.chars().next().map_or(false, |c| c.is_uppercase());
                if first_letter_uppercase && !self.may_uppercase.contains(*word) {
                    let lint_error =
                        LintError::from_node(ast, context, "Headings should be sentence case");
                    if let Some(lint_error) = lint_error {
                        errors.push(lint_error);
                    }
                }
            });
        }

        errors
    }
}
