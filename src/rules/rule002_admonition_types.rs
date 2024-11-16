use log::{trace, warn};
use markdown::mdast::Node;
use regex::Regex;
use supa_mdx_macros::RuleName;

use crate::{
    document::Location,
    errors::{LintError, LintLevel},
};

use super::{Rule, RuleContext, RuleName, RuleSettings};

#[derive(Debug, Default, Clone, RuleName)]
pub struct Rule002AdmonitionTypes {
    admonition_types: Vec<String>,
}

impl Rule for Rule002AdmonitionTypes {
    fn default_level(&self) -> LintLevel {
        LintLevel::Error
    }

    fn setup(&mut self, settings: Option<&RuleSettings>) {
        if let Some(settings) = settings {
            if let Some(vec) = settings.get_array_of_strings("admonition_types") {
                self.admonition_types = vec;
            }
        }
    }

    fn check(&self, ast: &Node, context: &RuleContext, level: LintLevel) -> Option<Vec<LintError>> {
        if !matches!(ast, Node::MdxJsxFlowElement(_)) {
            return None;
        };

        self.check_ast(ast, context, level)
            .map(|lint_error| vec![lint_error])
    }
}

impl Rule002AdmonitionTypes {
    fn message(&self, got: Option<&str>) -> String {
        format!(
            "Allowed admonition types are: {}.{}",
            self.admonition_types.join(", "),
            if let Some(got) = got {
                format!(" Got: \"{got}\".")
            } else {
                "".to_string()
            }
        )
    }

    fn check_ast(&self, node: &Node, context: &RuleContext, level: LintLevel) -> Option<LintError> {
        trace!("Checking ast for node: {node:?}");

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
        context: &RuleContext,
        level: LintLevel,
    ) -> Option<LintError> {
        LintError::from_node(node, context, &self.message(None), level)
    }

    fn create_lint_error_wrong_type(
        &self,
        node: &Node,
        context: &RuleContext,
        level: LintLevel,
        type_name: &str,
    ) -> Option<LintError> {
        let node_source = node
            .position()
            .map(|pos| &context.parse_result.source_content[pos.start.offset..pos.end.offset]);
        if let Some(node_source) = node_source {
            match Regex::new(r#"\b(type)\s*=\s*["']"#) {
                Ok(type_regex) => {
                    if let Some(match_result) =
                        type_regex.captures(node_source).and_then(|cap| cap.get(1))
                    {
                        let location = Location::maybe_from_node_with_offset(
                            node,
                            match_result.start(),
                            "type".len(),
                            context,
                        )
                        .unwrap(); // None is if node has no position, but we checked for that above

                        return Some(LintError {
                            level,
                            message: self.message(Some(type_name)),
                            location,
                            fix: None,
                        });
                    }
                }
                Err(_) => {
                    warn!("Error extracting type from admonition to fine-tune lint location: {node_source}");
                }
            }
        }

        LintError::from_node(node, context, &self.message(Some(type_name)), level)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        parser::parse,
        rules::{Rule, RuleContext},
        LintLevel,
    };

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
        let context = RuleContext::new(parse_result, None).unwrap();

        let admonition = context.parse_result.ast.children().unwrap().get(0).unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(result.is_some());
        assert!(result.as_ref().unwrap().len() == 1);
        let location = &result.as_ref().unwrap().get(0).unwrap().location;
        assert!(location.start().line.get() == 5);
        assert!(location.start().column.get() == 13);
        assert!(location.end().line.get() == 5);
        assert!(location.end().column.get() == 17);
    }

    #[test]
    fn test_admonition_types_missing_type() {
        let mdx = r#"<Admonition>
Some text.
</Admonition>"#;

        let rule = Rule002AdmonitionTypes::default();
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let admonition = context.parse_result.ast.children().unwrap().get(0).unwrap();
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
        let context = RuleContext::new(parse_result, None).unwrap();

        let admonition = context.parse_result.ast.children().unwrap().get(0).unwrap();
        let result = rule.check(admonition, &context, LintLevel::Error);

        assert!(result.is_none());
    }
}
