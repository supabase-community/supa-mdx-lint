use log::debug;
use markdown::mdast::Node;
use regex::Regex;
use std::sync::LazyLock;
use supa_mdx_macros::RuleName;

use crate::{
    context::Context,
    errors::{LintError, LintLevel},
    location::AdjustedRange,
};

use super::{Rule, RuleName, RuleSettings};

static ADMONITION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)<Admonition[^>]*>\s*\n\s*\n.*?\n\s*\n\s*</Admonition>").unwrap()
});

/// Admonition JSX tags must have empty line separation from their content.
///
/// ## Examples
///
/// ### Valid
///
/// ```mdx
/// <Admonition type="caution">
///
/// This is the content.
///
/// </Admonition>
/// ```
///
/// ### Invalid
///
/// ```mdx
/// <Admonition type="caution">
/// This is the content.
/// </Admonition>
/// ```
///
/// ## Rule Details
///
/// This rule enforces that Admonition components have proper spacing:
/// - Empty line after the opening `<Admonition>` tag
/// - Empty line before the closing `</Admonition>` tag
///
/// This ensures consistent formatting and improved readability of admonition content.
#[derive(Debug, Default, RuleName)]
pub struct Rule005AdmonitionNewlines;

impl Rule for Rule005AdmonitionNewlines {
    fn default_level(&self) -> LintLevel {
        LintLevel::Error
    }

    fn setup(&mut self, _settings: Option<&mut RuleSettings>) {
        // No configuration options for this rule
    }

    fn check(&self, ast: &Node, context: &Context, level: LintLevel) -> Option<Vec<LintError>> {
        if let Node::MdxJsxFlowElement(element) = ast {
            if element.name.as_ref().is_some_and(|name| name == "Admonition") {
                return self.check_admonition_newlines(element, context, level)
                    .map(|lint_error| vec![lint_error]);
            }
        }
        None
    }
}

impl Rule005AdmonitionNewlines {
    fn check_admonition_newlines(
        &self,
        element: &markdown::mdast::MdxJsxFlowElement,
        context: &Context,
        level: LintLevel,
    ) -> Option<LintError> {
        let position = element.position.as_ref()?;
        
        let rope = context.rope();
        
        debug!("Checking admonition at position: {:?}", position);
        
        // Find the opening and closing tag positions in the content
        let start_offset = position.start.offset;
        let end_offset = position.end.offset;
        
        // Extract only the admonition content slice from the rope
        let admonition_slice = rope.byte_slice(start_offset..end_offset);
        let admonition_content = admonition_slice.to_string();
        debug!("Admonition content: {:?}", admonition_content);
        
        // Check if the content matches the valid pattern
        if !self.has_proper_newlines(&admonition_content) {
            let range = AdjustedRange::from_unadjusted_position(position, context);
            return Some(LintError::builder()
                .rule(self.name())
                .message("Admonition must have empty lines between tags and content".to_string())
                .level(level)
                .location(range)
                .context(context)
                .build());
        }
        
        None
    }
    
    fn has_proper_newlines(&self, content: &str) -> bool {
        let matches = ADMONITION_PATTERN.is_match(content);
        debug!("Pattern match result for content {:?}: {}", content, matches);
        
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::parser::parse;

    #[test]
    fn test_valid_admonition_with_empty_lines() {
        let mdx = r#"<Admonition type="caution">

This is the content.

</Admonition>"#;

        let rule = Rule005AdmonitionNewlines::default();
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap()
            .get(0)
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(result.is_none(), "Expected no lint errors for valid admonition");
    }

    #[test]
    fn test_invalid_admonition_without_empty_lines() {
        let mdx = r#"<Admonition type="caution">
This is the content.
</Admonition>"#;

        let rule = Rule005AdmonitionNewlines::default();
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap()
            .get(0)
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(result.is_some(), "Expected lint error for invalid admonition");
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_admonition_missing_opening_empty_line() {
        let mdx = r#"<Admonition type="caution">
This is the content.

</Admonition>"#;

        let rule = Rule005AdmonitionNewlines::default();
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap()
            .get(0)
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(result.is_some(), "Expected lint error for missing opening empty line");
    }

    #[test]
    fn test_admonition_missing_closing_empty_line() {
        let mdx = r#"<Admonition type="caution">

This is the content.
</Admonition>"#;

        let rule = Rule005AdmonitionNewlines::default();
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap()
            .get(0)
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(result.is_some(), "Expected lint error for missing closing empty line");
    }
}
