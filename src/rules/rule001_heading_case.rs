use markdown::mdast::{Node, Text};
use regex::Regex;
use supa_mdx_macros::RuleName;

use crate::{
    document::{Location, Point, UnadjustedPoint},
    errors::{LintError, LintFix, LintFixReplace},
    utils::{split_first_word, HasChildren},
};

use super::{RegexSettings, Rule, RuleContext, RuleName, RuleSettings};

/// Internal flag for whether the first word still needs to be taken into
/// account when checking capitalization.
struct IncludesFirstWord(bool);

#[derive(Debug, Default, Clone, RuleName)]
pub struct Rule001HeadingCase {
    may_uppercase: Vec<Regex>,
    may_lowercase: Vec<Regex>,
}

impl Rule for Rule001HeadingCase {
    fn setup(&mut self, settings: Option<&RuleSettings>) {
        if let Some(settings) = settings {
            let regex_settings = RegexSettings {
                match_beginning: true,
                match_word_boundary_at_end: true,
            };

            if let Some(vec) = settings.get_array_of_regexes("may_uppercase", Some(&regex_settings))
            {
                self.may_uppercase = vec;
            }
            if let Some(vec) = settings.get_array_of_regexes("may_lowercase", Some(&regex_settings))
            {
                self.may_lowercase = vec;
            }
        }
    }

    fn check(&self, ast: &Node, context: &RuleContext) -> Option<Vec<LintError>> {
        if !matches!(ast, Node::Heading(_)) {
            return None;
        };

        let mut fixes: Vec<LintFix> = Vec::new();
        let mut includes_first_word = IncludesFirstWord(true);
        self.check_ast(ast, &mut fixes, &mut includes_first_word, context);

        let lint_error = if fixes.is_empty() {
            None
        } else {
            LintError::from_node_with_fix(ast, context, "Heading should be sentence case", fixes)
        };

        lint_error.map(|lint_error| vec![lint_error])
    }
}

#[derive(Debug)]
enum Case {
    Upper,
    Lower,
}

impl Rule001HeadingCase {
    fn check_text_sentence_case(
        &self,
        text: &Text,
        fixes: &mut Vec<LintFix>,
        includes_first_word: &mut IncludesFirstWord,
        context: &RuleContext,
    ) {
        let mut remaining_text = text.value.to_string();
        let mut char_index = 0;

        while !remaining_text.is_empty() {
            let trim_start = remaining_text.len() - remaining_text.trim_start().len();
            char_index += trim_start;
            remaining_text = remaining_text.trim_start().to_string();

            if remaining_text.is_empty() {
                break;
            }

            let first_char = remaining_text.chars().next().unwrap();

            if includes_first_word.0 {
                if first_char.is_lowercase() {
                    let (match_result, rest) = self.create_text_lint_fix(
                        &remaining_text,
                        text,
                        char_index,
                        Case::Lower,
                        context,
                    );
                    if let Some(fix) = match_result {
                        fixes.push(fix);
                    }
                    char_index += remaining_text.len() - rest.len();
                    remaining_text = rest;
                } else {
                    let (first_word, rest) = split_first_word(&remaining_text);
                    char_index += first_word.len();
                    remaining_text = rest.to_string();
                }

                includes_first_word.0 = false;
            } else if first_char.is_uppercase() {
                let (match_result, rest) = self.create_text_lint_fix(
                    &remaining_text,
                    text,
                    char_index,
                    Case::Upper,
                    context,
                );
                if let Some(fix) = match_result {
                    fixes.push(fix);
                }
                char_index += remaining_text.len() - rest.len();
                remaining_text = rest;
            } else {
                let (first_word, rest) = split_first_word(&remaining_text);
                char_index += first_word.len();
                remaining_text = rest.to_string();
            }
        }
    }

    fn create_text_lint_fix(
        &self,
        text: &str,
        node: &Text,
        index: usize,
        case: Case,
        context: &RuleContext,
    ) -> (Option<LintFix>, String) {
        let patterns = match case {
            Case::Upper => &self.may_uppercase,
            Case::Lower => &self.may_lowercase,
        };

        for pattern in patterns {
            if let Some(m) = pattern.find(text) {
                return (None, text[m.end()..].to_string());
            }
        }

        let (first_word, rest) = split_first_word(text);
        let replacement_word = match case {
            Case::Upper => first_word.to_lowercase(),
            Case::Lower => {
                let mut chars = first_word.chars();
                let first_char = chars.next().unwrap();
                first_char.to_uppercase().collect::<String>() + chars.as_str()
            }
        };

        let mut chars = node.value.chars();
        let mut text_to_move_over = String::new();
        for _ in 0..index {
            if let Some(ch) = chars.next() {
                text_to_move_over.push(ch);
            }
        }

        let start_point = node
            .position
            .as_ref()
            .map(|p| UnadjustedPoint::from(&p.start))
            .map(|mut p| {
                p.move_over_text(&text_to_move_over);
                p
            });
        let end_point = start_point.clone().map(|mut p| {
            p.move_over_text(first_word);
            p
        });

        match (start_point, end_point) {
            (Some(start), Some(end)) => (
                Some(LintFix::Replace(LintFixReplace {
                    location: Location::from_unadjusted_points(start, end, context),
                    text: replacement_word,
                })),
                rest.to_string(),
            ),
            _ => (None, rest.to_string()),
        }
    }

    fn check_ast(
        &self,
        node: &Node,
        fixes: &mut Vec<LintFix>,
        is_past_first_word: &mut IncludesFirstWord,
        context: &RuleContext,
    ) {
        fn check_children<T: HasChildren>(
            rule: &Rule001HeadingCase,
            node: &T,
            fixes: &mut Vec<LintFix>,
            is_past_first_word: &mut IncludesFirstWord,
            context: &RuleContext,
        ) {
            node.get_children()
                .iter()
                .for_each(|child| rule.check_ast(child, fixes, is_past_first_word, context));
        }

        match node {
            Node::Text(text) => {
                self.check_text_sentence_case(text, fixes, is_past_first_word, context)
            }
            Node::Emphasis(emphasis) => {
                check_children(self, emphasis, fixes, is_past_first_word, context)
            }
            Node::Link(link) => check_children(self, link, fixes, is_past_first_word, context),
            Node::LinkReference(link_reference) => {
                check_children(self, link_reference, fixes, is_past_first_word, context)
            }
            Node::Strong(strong) => {
                check_children(self, strong, fixes, is_past_first_word, context)
            }
            Node::Heading(heading) => {
                check_children(self, heading, fixes, is_past_first_word, context)
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use markdown::{
        mdast::{Heading, Text},
        unist::Position,
    };

    use crate::parser::ParseResult;

    use super::*;

    fn create_heading_node(content: &str, level: u8) -> Node {
        Node::Heading(Heading {
            depth: level,
            children: vec![Node::Text(Text {
                value: content.to_string(),
                position: Some(Position::new(
                    1,
                    3,
                    2,
                    1,
                    content.len() + 3,
                    content.len() + 2,
                )),
            })],
            position: Some(Position::new(
                1,
                1,
                2,
                1,
                content.len() + 3,
                content.len() + 2,
            )),
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

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let fixes = errors.get(0).unwrap().fix.clone();
        assert!(fixes.is_some());

        let fixes = fixes.unwrap();
        assert_eq!(fixes.len(), 1);

        let fix = fixes.get(0).unwrap();
        match fix {
            LintFix::Replace(fix) => {
                assert_eq!(fix.text, "This");
                assert_eq!(fix.location.start().line, NonZeroUsize::new(1).unwrap());
                assert_eq!(fix.location.start().column, NonZeroUsize::new(3).unwrap());
                assert_eq!(fix.location.start().offset, 2);
                assert_eq!(fix.location.end().line, NonZeroUsize::new(1).unwrap());
                assert_eq!(fix.location.end().column, NonZeroUsize::new(7).unwrap());
                assert_eq!(fix.location.end().offset, 6);
            }
            _ => panic!("Unexpected fix type"),
        }
    }

    #[test]
    fn test_uppercase_following_words() {
        let rule = Rule001HeadingCase::default();
        let heading = create_heading_node("This Should Fail", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_some());

        let errors = result.unwrap();
        assert_eq!(errors.len(), 1);

        let fixes = errors.get(0).unwrap().fix.clone();
        assert!(fixes.is_some());

        let fixes = fixes.unwrap();
        assert_eq!(fixes.len(), 2);

        let fix_one = fixes.get(0).unwrap();
        match fix_one {
            LintFix::Replace(fix) => {
                assert_eq!(fix.text, "should");
                assert_eq!(fix.location.start().line, NonZeroUsize::new(1).unwrap());
                assert_eq!(fix.location.start().column, NonZeroUsize::new(8).unwrap());
                assert_eq!(fix.location.start().offset, 7);
                assert_eq!(fix.location.end().line, NonZeroUsize::new(1).unwrap());
                assert_eq!(fix.location.end().column, NonZeroUsize::new(14).unwrap());
                assert_eq!(fix.location.end().offset, 13);
            }
            _ => panic!("Unexpected fix type"),
        }

        let fix_two = fixes.get(1).unwrap();
        match fix_two {
            LintFix::Replace(fix) => {
                assert_eq!(fix.text, "fail");
                assert_eq!(fix.location.start().line, NonZeroUsize::new(1).unwrap());
                assert_eq!(fix.location.start().column, NonZeroUsize::new(15).unwrap());
                assert_eq!(fix.location.start().offset, 14);
                assert_eq!(fix.location.end().line, NonZeroUsize::new(1).unwrap());
                assert_eq!(fix.location.end().column, NonZeroUsize::new(19).unwrap());
                assert_eq!(fix.location.end().offset, 18);
            }
            _ => panic!("Unexpected fix type"),
        }
    }

    #[test]
    fn test_may_uppercase() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["API"]);
        rule.setup(Some(&settings));

        let heading = create_heading_node("This is an API heading", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_may_lowercase() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_lowercase", vec!["the"]);
        rule.setup(Some(&settings));

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
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["New York City"]);
        rule.setup(Some(&settings));

        let heading = create_heading_node("This is about New York City", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_exception_matches() {
        let mut rule = Rule001HeadingCase::default();
        let settings =
            RuleSettings::with_array_of_strings("may_uppercase", vec!["New York", "New York City"]);
        rule.setup(Some(&settings));

        let heading = create_heading_node("This is about New York City", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_may_uppercase_partial_match() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["API"]);
        rule.setup(Some(&settings));

        let heading = create_heading_node("This is an API-related topic", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_may_lowercase_regex() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_lowercase", vec!["(the|a|an)"]);
        rule.setup(Some(&settings));

        let heading = create_heading_node("the quick brown fox", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }

    #[test]
    fn test_may_uppercase_regex_fails() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["[A-Z]{4,}"]);
        rule.setup(Some(&settings));

        let heading = create_heading_node("This is an API call", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(result.len(), 1);

        let error = result.get(0).unwrap();
        assert_eq!(error.fix.as_ref().unwrap().len(), 1);

        let fixes = error.fix.clone().unwrap();
        let fix = fixes.get(0).unwrap();
        match fix {
            LintFix::Replace(fix) => {
                assert_eq!(fix.text, "api");
                assert_eq!(fix.location.start().line, NonZeroUsize::new(1).unwrap());
                assert_eq!(fix.location.start().column, NonZeroUsize::new(14).unwrap());
                assert_eq!(fix.location.start().offset, 13);
                assert_eq!(fix.location.end().line, NonZeroUsize::new(1).unwrap());
                assert_eq!(fix.location.end().column, NonZeroUsize::new(17).unwrap());
                assert_eq!(fix.location.end().offset, 16);
            }
            _ => panic!("Unexpected fix type"),
        }
    }

    #[test]
    fn test_complex_heading() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["API", "OAuth"]);
        rule.setup(Some(&settings));

        let heading = create_heading_node("The basics of API authentication in OAuth", 1);
        let context = create_rule_context();

        let result = rule.check(&heading, &context);
        assert!(result.is_none());
    }
}
