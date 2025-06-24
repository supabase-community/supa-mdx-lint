use log::debug;
use markdown::mdast::Node;
use regex::Regex;
use std::sync::LazyLock;
use supa_mdx_macros::RuleName;

use crate::{
    context::Context,
    errors::{LintError, LintLevel},
    fix::{LintCorrection, LintCorrectionReplace},
    location::{AdjustedRange, DenormalizedLocation},
};

use super::{Rule, RuleName, RuleSettings};

#[derive(Debug)]
struct ErrorInfo {
    message: String,
}

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
                let mut fixes: Option<Vec<LintCorrection>> = None;
                if let Some(error_info) = self.check_admonition_newlines(element, context, &mut fixes) {
                    return LintError::from_node()
                        .node(ast)
                        .context(context)
                        .rule(self.name())
                        .level(level)
                        .message(&error_info.message)
                        .fix(fixes.unwrap_or_default())
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
        fixes: &mut Option<Vec<LintCorrection>>,
    ) -> Option<ErrorInfo> {
        let position = element.position.as_ref()?;
        // Convert to adjusted range immediately to handle frontmatter offsets
        let adjusted_range = AdjustedRange::from_unadjusted_position(position, context);
        
        let rope = context.rope();
        
        // Extract only the admonition content slice from the rope using adjusted offsets
        let range: std::ops::Range<usize> = adjusted_range.clone().into();
        let admonition_slice = rope.byte_slice(range);
        let admonition_content = admonition_slice.to_string();
        debug!("Admonition content: {:?}", admonition_content);
        
        // Check if the content matches the valid pattern
        if !self.has_proper_newlines(&admonition_content) {
            self.generate_fixes(&admonition_content, &adjusted_range, context, fixes);
            return Some(ErrorInfo {
                message: "Admonition must have empty lines between tags and content".to_string(),
            });
        }
        
        None
    }
    
    fn has_proper_newlines(&self, content: &str) -> bool {
        let matches = ADMONITION_PATTERN.is_match(content);
        debug!("Pattern match result for content {:?}: {}", content, matches);
        
        matches
    }
    
    fn generate_fixes(
        &self,
        content: &str,
        adjusted_range: &AdjustedRange,
        context: &Context,
        fixes: &mut Option<Vec<LintCorrection>>,
    ) {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return;
        }
        
        let mut fix_list = Vec::new();
        
        let opening_tag_line = 0;
        let closing_tag_line = lines.len() - 1;
        
        // Check if we need to add an empty line after the opening tag
        let needs_opening_newline = if lines.len() >= 2 {
            // Check if there's content immediately after the opening tag (no empty line)
            !lines[1].trim().is_empty()
        } else {
            false
        };
        
        // Check if we need to add an empty line before the closing tag  
        let needs_closing_newline = if closing_tag_line > 0 {
            // Check if there's content immediately before the closing tag (no empty line)
            !lines[closing_tag_line - 1].trim().is_empty()
        } else {
            false
        };
        
        // Add fix for opening newline
        if needs_opening_newline {
            // Position after the opening tag line + its newline
            let relative_offset = lines[opening_tag_line].len() + 1;
            
            let mut start_point = adjusted_range.start;
            start_point.increment(relative_offset);
            
            let location = DenormalizedLocation::from_offset_range(
                AdjustedRange::new(start_point, start_point),
                context,
            );
            
            fix_list.push(LintCorrection::Replace(LintCorrectionReplace {
                location,
                text: "\n".to_string(),
            }));
        }
        
        // Add fix for closing newline
        if needs_closing_newline {
            // Calculate relative position at the start of the closing tag line
            let mut relative_offset = 0;
            for (i, line) in lines.iter().enumerate() {
                if i == closing_tag_line {
                    break;
                }
                relative_offset += line.len() + 1; // +1 for newline character
            }
            
            let mut start_point = adjusted_range.start;
            start_point.increment(relative_offset);
            
            let location = DenormalizedLocation::from_offset_range(
                AdjustedRange::new(start_point, start_point),
                context,
            );
            
            fix_list.push(LintCorrection::Replace(LintCorrectionReplace {
                location,
                text: "\n".to_string(),
            }));
        }
        
        if !fix_list.is_empty() {
            *fixes = Some(fix_list);
        }
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

    #[test]
    fn test_auto_fix_missing_opening_empty_line() {
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
        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = &errors[0];
        assert!(error.fix.is_some(), "Expected fix to be present");
        
        let fixes = error.fix.as_ref().unwrap();
        assert_eq!(fixes.len(), 1, "Expected exactly one fix");
        
        match &fixes[0] {
            LintCorrection::Replace(fix) => {
                assert_eq!(fix.text, "\n", "Expected fix to add newline");
            }
            _ => panic!("Expected Replace fix"),
        }
    }

    #[test]
    fn test_auto_fix_missing_closing_empty_line() {
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
        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = &errors[0];
        assert!(error.fix.is_some(), "Expected fix to be present");
        
        let fixes = error.fix.as_ref().unwrap();
        assert_eq!(fixes.len(), 1, "Expected exactly one fix");
        
        match &fixes[0] {
            LintCorrection::Replace(fix) => {
                assert_eq!(fix.text, "\n", "Expected fix to add newline");
            }
            _ => panic!("Expected Replace fix"),
        }
    }

    #[test]
    fn test_auto_fix_missing_both_empty_lines() {
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

        assert!(result.is_some(), "Expected lint error for missing both empty lines");
        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let error = &errors[0];
        assert!(error.fix.is_some(), "Expected fix to be present");
        
        let fixes = error.fix.as_ref().unwrap();
        assert_eq!(fixes.len(), 2, "Expected exactly two fixes");
        
        // Both fixes should add newlines
        for fix in fixes {
            match fix {
                LintCorrection::Replace(fix) => {
                    assert_eq!(fix.text, "\n", "Expected fix to add newline");
                }
                _ => panic!("Expected Replace fix"),
            }
        }
    }

    #[test]
    fn test_no_fix_for_valid_admonition() {
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

        assert!(result.is_none(), "Expected no lint error for valid admonition");
    }

