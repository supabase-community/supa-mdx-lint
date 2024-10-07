use markdown::mdast::{Node, Text};
use regex::Regex;
use supa_mdx_macros::RuleName;

use crate::{
    errors::{LintError, LintFix},
    utils::{get_text_content, split_first_word, HasChildren},
    Fix,
};

use super::{RegexSetting, Rule, RuleContext, RuleName, RuleSettings};

#[derive(Debug, Default, RuleName)]
pub struct Rule001HeadingCase {
    may_uppercase: Vec<Regex>,
    may_lowercase: Vec<Regex>,
}

impl Rule for Rule001HeadingCase {
    fn setup(&mut self, settings: Option<&RuleSettings>) {
        if let Some(settings) = settings {
            if let Some(vec) =
                settings.get_array_of_regexes("may_uppercase", Some(RegexSetting::MatchBeginning))
            {
                self.may_uppercase = vec;
            }
            if let Some(vec) =
                settings.get_array_of_regexes("may_lowercase", Some(RegexSetting::MatchBeginning))
            {
                self.may_lowercase = vec;
            }
        }
    }

    fn check(&self, ast: &Node, context: &RuleContext) -> Option<Vec<LintError>> {
        if !matches!(ast, Node::Heading(_)) {
            return None;
        };

        let mut lint_result = None;
        let mut remaining_text = get_text_content(ast);
        let mut is_first_word = true;

        while !remaining_text.is_empty() {
            remaining_text = remaining_text.trim_start().to_string();

            if is_first_word {
                let is_lowercase = remaining_text
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_lowercase());
                if is_lowercase {
                    let match_result = self
                        .may_lowercase
                        .iter()
                        .find_map(|re| re.find(&remaining_text));
                    if let Some(match_result) = match_result {
                        remaining_text = remaining_text[match_result.end()..].to_string();
                    } else {
                        let lint_error =
                            LintError::from_node(ast, context, "Heading should be sentence case");
                        if let Some(lint_error) = lint_error {
                            lint_result = Some(vec![lint_error]);
                            break;
                        }
                    }
                } else {
                    let (_, rest) = split_first_word(&remaining_text);
                    remaining_text = rest.to_string();
                }

                is_first_word = false;
            } else {
                let is_uppercase = remaining_text
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_uppercase());
                if is_uppercase {
                    let match_result = self
                        .may_uppercase
                        .iter()
                        .find_map(|re| re.find(&remaining_text));
                    if let Some(match_result) = match_result {
                        remaining_text = remaining_text[match_result.end()..].to_string();
                    } else {
                        let lint_error =
                            LintError::from_node(ast, context, "Heading should be sentence case");
                        if let Some(lint_error) = lint_error {
                            lint_result = Some(vec![lint_error]);
                            break;
                        }
                    }
                } else {
                    let (_, rest) = split_first_word(&remaining_text);
                    remaining_text = rest.to_string();
                }
            }
        }

        if context.fix == Fix::True {
            let mut fixes = Vec::<LintFix>::new();
            let mut is_past_first_word = false;
            self.fix(ast, &mut fixes, &mut is_past_first_word, context);
        }

        lint_result
    }
}

impl Rule001HeadingCase {
    fn fix(
        &self,
        node: &Node,
        fixes: &mut Vec<LintFix>,
        is_past_first_word: &mut bool,
        context: &RuleContext,
    ) {
        fn fix_children<T: HasChildren>(
            rule: &Rule001HeadingCase,
            node: &T,
            fixes: &mut Vec<LintFix>,
            is_past_first_word: &mut bool,
            context: &RuleContext,
        ) {
            node.get_children()
                .iter()
                .for_each(|child| rule.fix(child, fixes, is_past_first_word, context));
        }

        match node {
            Node::Text(text) => {
                self.fix_text_sentence_case(text, fixes, is_past_first_word, context)
            }
            Node::Emphasis(emphasis) => {
                fix_children(self, emphasis, fixes, is_past_first_word, context)
            }
            Node::Link(link) => fix_children(self, link, fixes, is_past_first_word, context),
            Node::LinkReference(link_reference) => {
                fix_children(self, link_reference, fixes, is_past_first_word, context)
            }
            Node::Strong(strong) => fix_children(self, strong, fixes, is_past_first_word, context),
            Node::Heading(heading) => {
                fix_children(self, heading, fixes, is_past_first_word, context)
            }
            _ => {}
        }
    }

    fn fix_text_sentence_case(
        &self,
        node: &Text,
        fixes: &mut Vec<LintFix>,
        is_past_first_word: &mut bool,
        context: &RuleContext,
    ) {
        let text = &node.value;
        if text.is_empty() || text.trim().is_empty() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use markdown::{
        mdast::{Heading, Text},
        unist::Position,
    };

    use crate::{parser::ParseResult, Fix};

    use super::*;

    fn create_heading_node(content: &str, level: u8) -> Node {
        Node::Heading(Heading {
            depth: level,
            children: vec![Node::Text(Text {
                value: content.to_string(),
                // Dummy position to make sure the lint error is created
                position: Some(Position::new(1, 1, 0, 1, 2, 1)),
            })],
            // Dummy position to make sure the lint error is created
            position: Some(Position::new(1, 1, 0, 1, 2, 1)),
        })
    }

    fn create_rule_context() -> RuleContext {
        RuleContext {
            parse_result: ParseResult {
                ast: Node::Root(markdown::mdast::Root {
                    children: vec![],
                    position: None,
                }),
                frontmatter_lines: 0,
                frontmatter: None,
            },
            fix: Fix::False,
        }
    }

    #[test]
    fn test_correct_sentence_case() {
        let rule = Rule001HeadingCase::default();
        let heading = create_heading_node("This is a correct heading", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_lowercase_first_word() {
        let rule = Rule001HeadingCase::default();
        let heading = create_heading_node("this should fail", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_uppercase_following_words() {
        let rule = Rule001HeadingCase::default();
        let heading = create_heading_node("This Should Fail", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_may_uppercase() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = toml::Table::new();
        settings.insert(
            "may_uppercase".to_string(),
            toml::Value::Array(vec![toml::Value::String("API".to_string())]),
        );
        rule.setup(Some(&RuleSettings::new(settings)));

        let heading = create_heading_node("This is an API heading", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_may_lowercase() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = toml::Table::new();
        settings.insert(
            "may_lowercase".to_string(),
            toml::Value::Array(vec![toml::Value::String("the".to_string())]),
        );
        rule.setup(Some(&RuleSettings::new(settings)));

        let heading = create_heading_node("the quick brown fox", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_non_heading_node() {
        let rule = Rule001HeadingCase::default();
        let paragraph = Node::Paragraph(markdown::mdast::Paragraph {
            children: vec![],
            position: None,
        });
        let context = create_rule_context();

        let result = rule.check(&paragraph, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_may_uppercase_multi_word() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = toml::Table::new();
        settings.insert(
            "may_uppercase".to_string(),
            toml::Value::Array(vec![toml::Value::String(r"New York City".to_string())]),
        );
        rule.setup(Some(&RuleSettings::new(settings)));

        let heading = create_heading_node("This is about New York City", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_exception_matches() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = toml::Table::new();
        settings.insert(
            "may_uppercase".to_string(),
            toml::Value::Array(vec![
                toml::Value::String(r"New York".to_string()),
                toml::Value::String(r"New York City".to_string()),
            ]),
        );
        rule.setup(Some(&RuleSettings::new(settings)));

        let heading = create_heading_node("This is about New York City", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_may_uppercase_partial_match() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = toml::Table::new();
        settings.insert(
            "may_uppercase".to_string(),
            toml::Value::Array(vec![toml::Value::String(r"API".to_string())]),
        );
        rule.setup(Some(&RuleSettings::new(settings)));

        let heading = create_heading_node("This is an API-related topic", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_may_lowercase_regex() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = toml::Table::new();
        settings.insert(
            "may_lowercase".to_string(),
            toml::Value::Array(vec![toml::Value::String(r"(the|a|an)".to_string())]),
        );
        rule.setup(Some(&RuleSettings::new(settings)));

        let heading = create_heading_node("the quick brown fox", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_may_uppercase_regex_fails() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = toml::Table::new();
        settings.insert(
            "may_uppercase".to_string(),
            toml::Value::Array(vec![toml::Value::String(r"[A-Z]{4,}".to_string())]),
        );
        rule.setup(Some(&RuleSettings::new(settings)));

        let heading = create_heading_node("This is an API call", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);
    }

    #[test]
    fn test_complex_heading() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = toml::Table::new();
        settings.insert(
            "may_uppercase".to_string(),
            toml::Value::Array(vec![
                toml::Value::String(r"API".to_string()),
                toml::Value::String(r"OAuth".to_string()),
            ]),
        );
        rule.setup(Some(&RuleSettings::new(settings)));

        let heading = create_heading_node("The basics of API authentication in OAuth", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }
}
