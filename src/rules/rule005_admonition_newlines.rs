use log::debug;
use markdown::mdast::Node;
use regex::Regex;
use std::sync::LazyLock;
use supa_mdx_macros::RuleName;

use crate::{
    context::Context,
    errors::{LintError, LintLevel},
    fix::{LintCorrection, LintCorrectionInsert},
    location::{AdjustedRange, DenormalizedLocation},
};

use super::{Rule, RuleName, RuleSettings};

#[derive(Debug)]
struct ErrorInfo {
    message: String,
    fixes: Vec<LintCorrection>,
}

static OPENING_SEPARATION_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\r?\n[ \t]*\r?\n[ \t]*$").unwrap());
static AFTER_OPENING_SEPARATION_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\r?\n[ \t]*\r?\n").unwrap());

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
            if element
                .name
                .as_ref()
                .is_some_and(|name| name == "Admonition")
            {
                if let Some(error_info) = self.check_admonition_newlines(element, context) {
                    return LintError::from_node()
                        .node(ast)
                        .context(context)
                        .rule(self.name())
                        .level(level)
                        .message(&error_info.message)
                        .fix(error_info.fixes)
                        .call()
                        .map(|error| vec![error]);
                }
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
    ) -> Option<ErrorInfo> {
        // Check if this is a self-closing admonition (no children)
        if element.children.is_empty() {
            debug!("Skipping self-closing admonition");
            return None;
        }

        let position = element.position.as_ref()?;
        // Convert to adjusted range immediately to handle frontmatter offsets
        let adjusted_range = AdjustedRange::from_unadjusted_position(position, context);

        let rope = context.rope();

        // Extract only the admonition content slice from the rope using adjusted offsets
        let range: std::ops::Range<usize> = adjusted_range.clone().into();
        let admonition_slice = rope.byte_slice(range);
        let admonition_content = admonition_slice.to_string();
        debug!("Admonition content: {:?}", admonition_content);

        let opening_tag_end = find_opening_tag_end(&admonition_content)?;
        let content_after_opening = &admonition_content[opening_tag_end..];
        let closing_tag_offset = admonition_content.rfind("</Admonition>")?;
        let content_before_closing = &admonition_content[..closing_tag_offset];

        let has_opening_separation = starts_with_blank_line(content_after_opening);
        let has_closing_separation = OPENING_SEPARATION_PATTERN.is_match(content_before_closing);

        if !has_opening_separation || !has_closing_separation {
            let fixes = self.generate_fixes(
                &admonition_content,
                content_after_opening,
                opening_tag_end,
                content_before_closing,
                closing_tag_offset,
                &adjusted_range,
                context,
            );
            return Some(ErrorInfo {
                message: "Admonition must have empty lines between tags and content".to_string(),
                fixes,
            });
        }

        None
    }

    fn generate_fixes(
        &self,
        content: &str,
        content_after_opening: &str,
        opening_tag_end: usize,
        content_before_closing: &str,
        closing_tag_offset: usize,
        adjusted_range: &AdjustedRange,
        context: &Context,
    ) -> Vec<LintCorrection> {
        // Detect the line ending style used in the content
        let line_ending = if content.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };

        let mut fix_list = Vec::new();

        if !starts_with_blank_line(content_after_opening) {
            let (insertion_offset, missing_line_endings) =
                if content_after_opening.starts_with("\r\n") {
                    (opening_tag_end + 2, 1)
                } else if content_after_opening.starts_with('\n') {
                    (opening_tag_end + 1, 1)
                } else {
                    (opening_tag_end, 2)
                };
            let mut start_point = adjusted_range.start;
            start_point.increment(insertion_offset);
            let location = DenormalizedLocation::from_offset_range(
                AdjustedRange::new(start_point, start_point),
                context,
            );

            fix_list.push(LintCorrection::Insert(LintCorrectionInsert {
                location,
                text: line_ending.repeat(missing_line_endings),
            }));
        }

        if !OPENING_SEPARATION_PATTERN.is_match(content_before_closing) {
            let missing_line_endings = if content_before_closing.ends_with('\n') {
                1
            } else {
                2
            };
            let mut start_point = adjusted_range.start;
            start_point.increment(closing_tag_offset);
            let location = DenormalizedLocation::from_offset_range(
                AdjustedRange::new(start_point, start_point),
                context,
            );

            fix_list.push(LintCorrection::Insert(LintCorrectionInsert {
                location,
                text: line_ending.repeat(missing_line_endings),
            }));
        }

        fix_list
    }
}

fn starts_with_blank_line(content: &str) -> bool {
    AFTER_OPENING_SEPARATION_PATTERN.is_match(content)
}

fn find_opening_tag_end(content: &str) -> Option<usize> {
    let mut brace_depth: usize = 0;
    let mut quote = None;
    let mut escaped = false;

    for (offset, character) in content.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        if character == '\\' && quote.is_some() {
            escaped = true;
            continue;
        }

        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            }
            continue;
        }

        match character {
            '"' | '\'' | '`' => quote = Some(character),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            '>' if brace_depth == 0 => return Some(offset + character.len_utf8()),
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Context;
    use crate::parser::parse;

    #[test]
    fn test_rule005_valid_admonition_with_empty_lines() {
        let mdx = r#"<Admonition type="caution">

This is the content.

</Admonition>"#;

        let rule = Rule005AdmonitionNewlines;
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap().first()
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(
            result.is_none(),
            "Expected no lint errors for valid admonition"
        );
    }

    #[test]
    fn test_rule005_valid_admonition_with_jsx_action() {
        let mdx = r#"<Admonition
  type="note"
  actions={
    <Button>Continue</Button>
  }
>

This is the content.

</Admonition>"#;

        let rule = Rule005AdmonitionNewlines;
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
            .first()
            .unwrap();

        assert!(rule
            .check(admonition, &context, LintLevel::Error)
            .is_none());
    }

    #[test]
    fn test_rule005_invalid_admonition_without_empty_lines() {
        let mdx = r#"<Admonition type="caution">
This is the content.
</Admonition>"#;

        let rule = Rule005AdmonitionNewlines;
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap().first()
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(
            result.is_some(),
            "Expected lint error for invalid admonition"
        );
        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = &errors[0];
        assert_eq!(error.location.start.row, 0);
        assert_eq!(error.location.start.column, 0);
    }

    #[test]
    fn test_rule005_admonition_missing_opening_empty_line() {
        let mdx = r#"<Admonition type="caution">
This is the content.

</Admonition>"#;

        let rule = Rule005AdmonitionNewlines;
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap().first()
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(
            result.is_some(),
            "Expected lint error for missing opening empty line"
        );
        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = &errors[0];
        assert_eq!(error.location.start.row, 0);
        assert_eq!(error.location.start.column, 0);
    }

    #[test]
    fn test_rule005_admonition_missing_closing_empty_line() {
        let mdx = r#"<Admonition type="caution">

This is the content.
</Admonition>"#;

        let rule = Rule005AdmonitionNewlines;
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap().first()
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(
            result.is_some(),
            "Expected lint error for missing closing empty line"
        );
        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = &errors[0];
        assert_eq!(error.location.start.row, 0);
        assert_eq!(error.location.start.column, 0);
    }

    #[test]
    fn test_rule005_auto_fix_missing_opening_empty_line() {
        let mdx = r#"<Admonition type="caution">
This is the content.

</Admonition>"#;

        let rule = Rule005AdmonitionNewlines;
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap().first()
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(
            result.is_some(),
            "Expected lint error for missing opening empty line"
        );
        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = &errors[0];
        assert_eq!(error.location.start.row, 0);
        assert_eq!(error.location.start.column, 0);
        assert!(error.fix.is_some(), "Expected fix to be present");

        let fixes = error.fix.as_ref().unwrap();
        assert_eq!(fixes.len(), 1, "Expected exactly one fix");

        match &fixes[0] {
            LintCorrection::Insert(fix) => {
                assert_eq!(fix.text, "\n", "Expected fix to add newline");
                assert_eq!(fix.location.start.row, 1);
                assert_eq!(fix.location.start.column, 0);
            }
            _ => panic!("Expected Insert fix"),
        }
    }

    #[test]
    fn test_rule005_auto_fix_missing_closing_empty_line() {
        let mdx = r#"<Admonition type="caution">

This is the content.
</Admonition>"#;

        let rule = Rule005AdmonitionNewlines;
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap().first()
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(
            result.is_some(),
            "Expected lint error for missing closing empty line"
        );
        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = &errors[0];
        assert_eq!(error.location.start.row, 0);
        assert_eq!(error.location.start.column, 0);
        assert!(error.fix.is_some(), "Expected fix to be present");

        let fixes = error.fix.as_ref().unwrap();
        assert_eq!(fixes.len(), 1, "Expected exactly one fix");

        match &fixes[0] {
            LintCorrection::Insert(fix) => {
                assert_eq!(fix.text, "\n", "Expected fix to add newline");
                assert_eq!(fix.location.start.row, 3);
                assert_eq!(fix.location.start.column, 0);
            }
            _ => panic!("Expected Insert fix"),
        }
    }

    #[test]
    fn test_rule005_auto_fix_missing_both_empty_lines() {
        let mdx = r#"<Admonition type="caution">
This is the content.
</Admonition>"#;

        let rule = Rule005AdmonitionNewlines;
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap().first()
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(
            result.is_some(),
            "Expected lint error for missing both empty lines"
        );
        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = &errors[0];
        assert_eq!(error.location.start.row, 0);
        assert_eq!(error.location.start.column, 0);
        assert!(error.fix.is_some(), "Expected fix to be present");

        let fixes = error.fix.as_ref().unwrap();
        assert_eq!(fixes.len(), 2, "Expected exactly two fixes");

        // First fix should be for opening newline
        match &fixes[0] {
            LintCorrection::Insert(fix) => {
                assert_eq!(fix.text, "\n", "Expected fix to add newline");
                assert_eq!(fix.location.start.row, 1);
                assert_eq!(fix.location.start.column, 0);
            }
            _ => panic!("Expected Insert fix"),
        }

        // Second fix should be for closing newline
        match &fixes[1] {
            LintCorrection::Insert(fix) => {
                assert_eq!(fix.text, "\n", "Expected fix to add newline");
                assert_eq!(fix.location.start.row, 2);
                assert_eq!(fix.location.start.column, 0);
            }
            _ => panic!("Expected Insert fix"),
        }
    }

    #[test]
    fn test_rule005_no_fix_for_valid_admonition() {
        let mdx = r#"<Admonition type="caution">

This is the content.

</Admonition>"#;

        let rule = Rule005AdmonitionNewlines;
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap().first()
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(
            result.is_none(),
            "Expected no lint error for valid admonition"
        );
    }

    #[test]
    fn test_rule005_self_closing_admonition() {
        let mdx =
            r#"<Admonition type="note" label="Data changes are not merged into production." />"#;

        let rule = Rule005AdmonitionNewlines;
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let admonition = context
            .parse_result
            .ast()
            .children()
            .unwrap().first()
            .unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(
            result.is_none(),
            "Expected no lint error for self-closing admonition"
        );
    }
}
