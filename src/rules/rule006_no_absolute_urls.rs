use markdown::mdast::{Image, Link, Node};
use supa_mdx_macros::RuleName;

use crate::{
    context::Context,
    errors::{LintError, LintLevel},
    fix::LintCorrectionReplace,
    location::{AdjustedRange, DenormalizedLocation},
};

use super::{Rule, RuleName, RuleSettings};

/// Links and images should use relative URLs instead of absolute URLs that match the configured base URL.
///
/// ## Examples
///
/// ### Valid
///
/// ```markdown
/// [Documentation](/docs/auth)
/// ![Logo](/images/logo.png)
/// ```
///
/// ### Invalid (assuming base_url is `https://supabase.com`)
///
/// ```markdown
/// [Documentation](https://supabase.com/docs/auth)
/// ![Logo](https://supabase.com/images/logo.png)
/// ```
///
/// ## Configuration
///
/// Configure the base URL via the `base_url` setting in your configuration file:
///
/// ```toml
/// [rule006_no_absolute_urls]
/// base_url = "https://supabase.com"
/// ```
#[derive(Debug, Default, RuleName)]
pub struct Rule006NoAbsoluteUrls {
    base_url: Option<String>,
}

impl Rule for Rule006NoAbsoluteUrls {
    fn default_level(&self) -> LintLevel {
        LintLevel::Error
    }

    fn setup(&mut self, settings: Option<&mut RuleSettings>) {
        if let Some(settings) = settings {
            if let Some(toml::Value::String(base_url)) = settings.0.get("base_url") {
                let base_url = base_url.trim_end_matches('/').to_string();
                self.base_url = Some(base_url);
            }
        }
    }

    fn check(&self, ast: &Node, context: &Context, level: LintLevel) -> Option<Vec<LintError>> {
        let url = match ast {
            Node::Link(link) => &link.url,
            Node::Image(image) => &image.url,
            _ => return None,
        };

        // Skip if no base URL is configured
        let base_url = self.base_url.as_ref()?;

        if url.starts_with(base_url) {
            let relative_path = &url[base_url.len()..];

            let relative_path = if relative_path.starts_with('/') {
                relative_path
            } else {
                // This shouldn't happen, but fail gracefully
                return None;
            };

            if let Some(url_location) = self.find_url_location(ast, context) {
                let correction = LintCorrectionReplace {
                    location: url_location,
                    text: relative_path.to_string(),
                };

                let error = LintError::from_node()
                    .node(ast)
                    .context(context)
                    .rule(self.name())
                    .level(level)
                    .message(&self.message(url, relative_path))
                    .fix(vec![crate::fix::LintCorrection::Replace(correction)])
                    .call();

                return error.map(|err| vec![err]);
            }
        }

        None
    }
}

impl Rule006NoAbsoluteUrls {
    fn message(&self, absolute_url: &str, relative_url: &str) -> String {
        format!(
            "Use relative URL '{}' instead of absolute URL '{}'",
            relative_url, absolute_url
        )
    }

    /// Find the exact location of the URL within the markdown text
    /// This method specifically looks for the URL within the parentheses portion
    /// to avoid matching URLs that might appear in the display text.
    fn find_url_location(&self, ast: &Node, context: &Context) -> Option<DenormalizedLocation> {
        let (url, node_position) = match ast {
            Node::Link(Link { url, position, .. }) => (url, position.as_ref()?),
            Node::Image(Image { url, position, .. }) => (url, position.as_ref()?),
            _ => return None,
        };

        let node_range = AdjustedRange::from_unadjusted_position(node_position, context);
        let node_start_offset: usize = node_range.start.into();
        let node_text = context
            .rope()
            .byte_slice(Into::<std::ops::Range<usize>>::into(node_range));
        let node_text_str = node_text.to_string();

        // Find the URL specifically within the parentheses portion
        // For links: [text](URL) - look for the last opening paren, then find URL after it
        // For images: ![alt](URL) - look for the last opening paren, then find URL after it
        if let Some(paren_start) = node_text_str.rfind('(') {
            // Look for the URL after the opening parenthesis
            let after_paren = &node_text_str[paren_start + 1..];
            if let Some(url_in_parens) = after_paren.find(url) {
                // Make sure this is at the start of the parentheses content (accounting for whitespace)
                let before_url = &after_paren[..url_in_parens];
                if before_url.trim().is_empty() {
                    let url_start_in_text = paren_start + 1 + url_in_parens;
                    let url_start_offset = node_start_offset + url_start_in_text;
                    let url_end_offset = url_start_offset + url.len();

                    let url_range =
                        AdjustedRange::new(url_start_offset.into(), url_end_offset.into());
                    return Some(DenormalizedLocation::from_offset_range(url_range, context));
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{context::Context, parser::parse};

    fn find_link_node(node: &Node) -> Option<&Node> {
        match node {
            Node::Link(_) => Some(node),
            Node::Image(_) => Some(node),
            _ => {
                if let Some(children) = node.children() {
                    for child in children {
                        if let Some(found) = find_link_node(child) {
                            return Some(found);
                        }
                    }
                }
                None
            }
        }
    }

    #[test]
    fn test_absolute_link_with_matching_base_url() {
        let mut rule = Rule006NoAbsoluteUrls::default();
        let mut settings = super::super::RuleSettings::from_key_value(
            "base_url",
            toml::Value::String("https://supabase.com".to_string()),
        );
        rule.setup(Some(&mut settings));

        let markdown = "[Documentation](https://supabase.com/docs/auth)";
        let parse_result = parse(markdown).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let link_node = find_link_node(parse_result.ast()).expect("Should find a link node");
        let errors = rule.check(link_node, &context, LintLevel::Error);
        assert!(errors.is_some());

        let errors = errors.unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("/docs/auth"));
    }

    #[test]
    fn test_absolute_link_with_non_matching_base_url() {
        let mut rule = Rule006NoAbsoluteUrls::default();
        let mut settings = super::super::RuleSettings::from_key_value(
            "base_url",
            toml::Value::String("https://supabase.com".to_string()),
        );
        rule.setup(Some(&mut settings));

        let markdown = "[External](https://example.com/docs)";
        let parse_result = parse(markdown).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let link_node = find_link_node(parse_result.ast()).expect("Should find a link node");
        let errors = rule.check(link_node, &context, LintLevel::Error);
        assert!(errors.is_none());
    }

    #[test]
    fn test_relative_link() {
        let mut rule = Rule006NoAbsoluteUrls::default();
        let mut settings = super::super::RuleSettings::from_key_value(
            "base_url",
            toml::Value::String("https://supabase.com".to_string()),
        );
        rule.setup(Some(&mut settings));

        let markdown = "[Documentation](/docs/auth)";
        let parse_result = parse(markdown).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let link_node = find_link_node(parse_result.ast()).expect("Should find a link node");
        let errors = rule.check(link_node, &context, LintLevel::Error);
        assert!(errors.is_none());
    }

    #[test]
    fn test_image_with_absolute_url() {
        let mut rule = Rule006NoAbsoluteUrls::default();
        let mut settings = super::super::RuleSettings::from_key_value(
            "base_url",
            toml::Value::String("https://supabase.com".to_string()),
        );
        rule.setup(Some(&mut settings));

        let markdown = "![Logo](https://supabase.com/images/logo.png)";
        let parse_result = parse(markdown).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let image_node = find_link_node(parse_result.ast()).expect("Should find an image node");
        let errors = rule.check(image_node, &context, LintLevel::Error);
        assert!(errors.is_some());

        let errors = errors.unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("/images/logo.png"));
    }

    #[test]
    fn test_no_base_url_configured() {
        let rule = Rule006NoAbsoluteUrls::default();

        let markdown = "[Documentation](https://supabase.com/docs/auth)";
        let parse_result = parse(markdown).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let link_node = find_link_node(parse_result.ast()).expect("Should find a link node");
        let errors = rule.check(link_node, &context, LintLevel::Error);
        assert!(errors.is_none());
    }

    #[test]
    fn test_url_in_display_text_and_href() {
        let mut rule = Rule006NoAbsoluteUrls::default();
        let mut settings = super::super::RuleSettings::from_key_value(
            "base_url",
            toml::Value::String("https://supabase.com".to_string()),
        );
        rule.setup(Some(&mut settings));

        // URL appears in both display text and href - should only fix the href
        let markdown = "[https://supabase.com](https://supabase.com/docs/auth)";
        let parse_result = parse(markdown).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let link_node = find_link_node(parse_result.ast()).expect("Should find a link node");
        let errors = rule.check(link_node, &context, LintLevel::Error);
        assert!(errors.is_some());

        let errors = errors.unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("/docs/auth"));

        // Verify the fix would only replace the href part  
        assert!(errors[0].fix.is_some(), "Expected fix to be present");
        let fixes = errors[0].fix.as_ref().unwrap();
        assert_eq!(fixes.len(), 1);
        if let crate::fix::LintCorrection::Replace(replace_fix) = &fixes[0] {
            assert_eq!(replace_fix.text(), "/docs/auth");
            
            // Verify the location is correct - should target only the URL in parentheses
            let location = &replace_fix.location;
            
            // The original text is "[https://supabase.com](https://supabase.com/docs/auth)"  
            // Position of the URL in parentheses starts at index 23 and ends at 53
            // [https://supabase.com](https://supabase.com/docs/auth)
            // 012345678901234567890123456789012345678901234567890123456789
            //                        ^                             ^  
            //                        23                            53
            let expected_start = 23_usize;
            let expected_end = 53_usize;
            
            let actual_start: usize = location.offset_range.start.into();
            let actual_end: usize = location.offset_range.end.into();
            assert_eq!(actual_start, expected_start);
            assert_eq!(actual_end, expected_end);
        } else {
            panic!("Expected Replace correction");
        }
    }

    #[test]
    fn test_url_only_in_display_text() {
        let mut rule = Rule006NoAbsoluteUrls::default();
        let mut settings = super::super::RuleSettings::from_key_value(
            "base_url",
            toml::Value::String("https://supabase.com".to_string()),
        );
        rule.setup(Some(&mut settings));

        // URL only in display text, href is different - should not trigger
        let markdown = "[https://supabase.com](https://example.com/docs)";
        let parse_result = parse(markdown).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let link_node = find_link_node(parse_result.ast()).expect("Should find a link node");
        let errors = rule.check(link_node, &context, LintLevel::Error);
        assert!(errors.is_none());
    }

    #[test]
    fn test_image_url_in_alt_text_and_src() {
        let mut rule = Rule006NoAbsoluteUrls::default();
        let mut settings = super::super::RuleSettings::from_key_value(
            "base_url",
            toml::Value::String("https://supabase.com".to_string()),
        );
        rule.setup(Some(&mut settings));

        // URL appears in both alt text and src - should only fix the src
        let markdown = "![https://supabase.com](https://supabase.com/logo.png)";
        let parse_result = parse(markdown).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let image_node = find_link_node(parse_result.ast()).expect("Should find an image node");
        let errors = rule.check(image_node, &context, LintLevel::Error);
        assert!(errors.is_some());

        let errors = errors.unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("/logo.png"));
        
        // Verify the fix would only replace the src part
        assert!(errors[0].fix.is_some(), "Expected fix to be present");
        let fixes = errors[0].fix.as_ref().unwrap();
        assert_eq!(fixes.len(), 1);
        if let crate::fix::LintCorrection::Replace(replace_fix) = &fixes[0] {
            assert_eq!(replace_fix.text(), "/logo.png");
            
            // Verify the location is correct - should target only the URL in parentheses
            let location = &replace_fix.location;
            
            // The original text is "![https://supabase.com](https://supabase.com/logo.png)"
            // Position of the URL in parentheses starts at index 24 and ends at 53
            // ![https://supabase.com](https://supabase.com/logo.png)
            // 012345678901234567890123456789012345678901234567890123456789
            //                         ^                            ^
            //                         24                           53
            let expected_start = 24_usize;
            let expected_end = 53_usize;
            
            let actual_start: usize = location.offset_range.start.into();
            let actual_end: usize = location.offset_range.end.into();
            assert_eq!(actual_start, expected_start);
            assert_eq!(actual_end, expected_end);
        } else {
            panic!("Expected Replace correction");
        }
    }
}
