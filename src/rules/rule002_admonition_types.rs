use log::{trace, warn};
use markdown::mdast::Node;
use regex::Regex;
use supa_mdx_macros::RuleName;

use crate::{
    context::Context,
    errors::{LintError, LintLevel},
    location::{AdjustedOffset, AdjustedRange, DenormalizedLocation},
};

use super::{Rule, RuleName, RuleSettings};

/// Admonitions must have a single valid type.
///
/// ## Configuration
///
/// Valid admonition types are enumerated in the `admonition_types` array.
///
/// See an  [example from the Supabase repo](https://github.com/supabase/supabase/blob/master/supa-mdx-lint.config.toml#L12).
#[derive(Debug, Default, RuleName)]
pub struct Rule002AdmonitionTypes {
    admonition_types: Vec<String>,
}

impl Rule for Rule002AdmonitionTypes {
    fn default_level(&self) -> LintLevel {
        LintLevel::Error
    }

    fn setup(&mut self, settings: Option<&mut RuleSettings>) {
        if let Some(settings) = settings {
            if let Some(vec) = settings.get_array_of_strings("admonition_types") {
                self.admonition_types = vec;
            }
        }
    }

    fn check(&self, ast: &Node, context: &Context, level: LintLevel) -> Option<Vec<LintError>> {
        if !matches!(ast, Node::MdxJsxFlowElement(_)) {
            return None;
        };

        self.check_ast(ast, context, level)
            .map(|lint_error| vec![lint_error])
    }
}

impl Rule002AdmonitionTypes {
    fn message(&self, got: Option<&str>) -> String {
        match got {
            Some(got) => format!(
                "Allowed admonition types are: {}. Got: \"{got}\".",
                self.admonition_types.join(", "),
            ),
            None => "Missing admonition type.".to_string(),
        }
    }

    fn check_ast(&self, node: &Node, context: &Context, level: LintLevel) -> Option<LintError> {
        trace!("Checking AST for node: {node:#?}");

        match node {
            Node::MdxJsxFlowElement(element)
                if element
                    .name
                    .as_ref()
                    .map_or(false, |name| name == "Admonition") =>
            {
                for attr in &element.attributes {
                    match attr {
                        markdown::mdast::AttributeContent::Property(mdx_jsx_attribute)
                            if mdx_jsx_attribute.name == "type" =>
                        {
                            if let Some(markdown::mdast::AttributeValue::Literal(type_name)) =
                                &mdx_jsx_attribute.value
                            {
                                let allowed_type_name = self.admonition_types.contains(type_name);
                                if allowed_type_name {
                                    return None;
                                } else {
                                    return self.create_lint_error_wrong_type(
                                        node, context, level, type_name,
                                    );
                                }
                            }
                        }
                        _ => {}
                    }
                }

                self.create_lint_error_missing_type(node, context, level)
            }
            _ => None,
        }
    }

    fn create_lint_error_missing_type(
        &self,
        node: &Node,
        context: &Context,
        level: LintLevel,
    ) -> Option<LintError> {
        LintError::from_node()
            .node(node)
            .context(context)
            .rule(self.name())
            .message(&self.message(None))
            .level(level)
            .call()
    }

    fn create_lint_error_wrong_type(
        &self,
        node: &Node,
        context: &Context,
        level: LintLevel,
        type_name: &str,
    ) -> Option<LintError> {
        let node_source = node
            .position()
            .map(|pos| {
                let start = AdjustedOffset::from_unist(&pos.start, context.content_start_offset());
                let end = AdjustedOffset::from_unist(&pos.end, context.content_start_offset());
                context
                    .rope()
                    .byte_slice(Into::<usize>::into(start)..Into::<usize>::into(end))
            })
            .map(|slice| slice.to_string());
        if let Some(node_source) = node_source {
            match Regex::new(r#"\b(type)\s*=\s*["']"#) {
                Ok(type_regex) => {
                    if let Some(match_result) =
                        type_regex.captures(&node_source).and_then(|cap| cap.get(1))
                    {
                        let mut start_point = AdjustedOffset::from_unist(
                            &node.position().unwrap().start,
                            context.content_start_offset(),
                        );
                        start_point.increment(match_result.start());
                        let mut end_point = start_point;
                        end_point.increment("type".len());

                        let range = AdjustedRange::new(start_point, end_point);
                        let location = DenormalizedLocation::from_offset_range(range, context);

                        return Some(
                            LintError::from_raw_location()
                                .rule(self.name())
                                .level(level)
                                .message(self.message(Some(type_name)))
                                .location(location)
                                .call(),
                        );
                    }
                }
                Err(_) => {
                    warn!("Error extracting type from admonition to fine-tune lint location: {node_source}");
                }
            }
        }

        LintError::from_node()
            .node(node)
            .context(context)
            .rule(self.name())
            .message(&self.message(Some(type_name)))
            .level(level)
            .call()
    }
}

#[cfg(test)]
mod tests {
    use crate::{context::Context, parser::parse, rules::Rule, LintLevel};

    use super::Rule002AdmonitionTypes;

    #[test]
    fn test_admonition_types_wrong_type() {
        let mdx = r#"---
title: Hello
---

<Admonition type="wrong">
Some text.
</Admonition>"#;

        let rule = Rule002AdmonitionTypes::default();
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

        assert!(result.is_some());
        assert!(result.as_ref().unwrap().len() == 1);
        let location = &result.as_ref().unwrap().get(0).unwrap().location;
        assert!(location.start.row == 4);
        assert!(location.start.column == 12);
        assert!(location.end.row == 4);
        assert!(location.end.column == 16);
    }

    #[test]
    fn test_admonition_types_missing_type() {
        let mdx = r#"<Admonition>
Some text.
</Admonition>"#;

        let rule = Rule002AdmonitionTypes::default();
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

        assert!(result.is_some());
        assert!(result.unwrap().len() == 1);
    }

    #[test]
    fn test_admonition_types_correct_type() {
        let mdx = r#"<Admonition type="note">
Some text.
</Admonition>"#;

        let mut rule = Rule002AdmonitionTypes::default();
        rule.admonition_types = vec!["note".to_string()];
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

        assert!(result.is_none());
    }
}
