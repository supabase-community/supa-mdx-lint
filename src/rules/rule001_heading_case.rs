use log::trace;
use markdown::mdast::{Node, Text};
use regex::Regex;
use supa_mdx_macros::RuleName;

use crate::{
    errors::{LintError, LintLevel},
    fix::{LintFix, LintFixReplace},
    geometry::{AdjustedOffset, AdjustedRange, DenormalizedLocation},
    utils::{split_first_word_at_whitespace_and_colons, HasChildren},
};

use super::{RegexSettings, Rule, RuleContext, RuleName, RuleSettings};

#[derive(Debug)]
struct NextWordCapital(bool);

#[derive(Debug, Default, Clone, RuleName)]
pub struct Rule001HeadingCase {
    may_uppercase: Vec<Regex>,
    may_lowercase: Vec<Regex>,
}

impl Rule for Rule001HeadingCase {
    fn default_level(&self) -> LintLevel {
        LintLevel::Error
    }

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

    fn check(&self, ast: &Node, context: &RuleContext, level: LintLevel) -> Option<Vec<LintError>> {
        if !matches!(ast, Node::Heading(_)) {
            return None;
        };

        let mut fixes: Vec<LintFix> = Vec::new();
        let mut next_word_capital = NextWordCapital(true);
        self.check_ast(ast, &mut fixes, &mut next_word_capital, context);

        let lint_error = if fixes.is_empty() {
            None
        } else {
            LintError::from_node_with_fix(ast, context, self.name(), &self.message(), level, fixes)
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
    fn message(&self) -> String {
        "Heading should be sentence case".to_string()
    }

    fn check_text_sentence_case(
        &self,
        text: &Text,
        fixes: &mut Vec<LintFix>,
        next_word_capital: &mut NextWordCapital,
        context: &RuleContext,
    ) {
        let mut remaining_text = text.value.as_str();
        let mut char_index = 0;

        while !remaining_text.is_empty() {
            let mut chars = remaining_text.chars();
            let mut next_alphabetic_index = 0;
            while let Some(c) = chars.next().and_then(|c| {
                if c.is_ascii_alphabetic() {
                    None
                } else {
                    Some(c)
                }
            }) {
                if c == ':' {
                    next_word_capital.0 = true;
                } else if !c.is_whitespace() {
                    next_word_capital.0 = false;
                }
                next_alphabetic_index += 1;
            }

            remaining_text = &remaining_text[next_alphabetic_index..];
            char_index += next_alphabetic_index;

            if remaining_text.is_empty() {
                break;
            }

            trace!("[Rule001HeadingCase] Checking remaining text \"{remaining_text}\" at index {char_index} with {next_word_capital:?}");

            let first_char = remaining_text.chars().next().unwrap();

            if next_word_capital.0 {
                if first_char.is_lowercase() {
                    let (match_result, rest, split_on_colon) = self.create_text_lint_fix(
                        remaining_text,
                        text,
                        char_index,
                        Case::Lower,
                        context,
                    );
                    if let Some(fix) = match_result {
                        fixes.push(fix);
                    }
                    if !split_on_colon {
                        next_word_capital.0 = false;
                    }
                    char_index += remaining_text.len() - rest.len();
                    remaining_text = rest;
                } else {
                    let exception = self
                        .may_uppercase
                        .iter()
                        .find(|pattern| pattern.is_match(remaining_text));
                    if exception.is_some() {
                        let match_result = exception.unwrap().find(remaining_text).unwrap();
                        remaining_text = &remaining_text[match_result.end()..];
                        char_index += match_result.len();
                        if !remaining_text.starts_with(':') {
                            next_word_capital.0 = false;
                        }
                    } else {
                        let (first_word, rest, split_on_colon) =
                            split_first_word_at_whitespace_and_colons(remaining_text);
                        if !split_on_colon {
                            next_word_capital.0 = false;
                        }
                        char_index += first_word.len();
                        remaining_text = rest;
                    }
                }
            } else if first_char.is_uppercase() {
                let (match_result, rest, split_on_colon) = self.create_text_lint_fix(
                    remaining_text,
                    text,
                    char_index,
                    Case::Upper,
                    context,
                );
                if let Some(fix) = match_result {
                    fixes.push(fix);
                }
                if split_on_colon {
                    next_word_capital.0 = true;
                }
                char_index += remaining_text.len() - rest.len();
                remaining_text = rest;
            } else {
                let (first_word, rest, split_on_colon) =
                    split_first_word_at_whitespace_and_colons(remaining_text);
                if split_on_colon {
                    next_word_capital.0 = true;
                }
                char_index += first_word.len();
                remaining_text = rest;
            }
        }
    }

    fn create_text_lint_fix<'node>(
        &self,
        text: &'node str,
        node: &'node Text,
        index: usize,
        case: Case,
        context: &RuleContext,
    ) -> (Option<LintFix>, &'node str, bool) {
        let patterns = match case {
            Case::Upper => &self.may_uppercase,
            Case::Lower => &self.may_lowercase,
        };
        trace!(
            "Checking text case for {:?} with patterns {:#?}",
            text,
            patterns
        );

        for pattern in patterns {
            if let Some(m) = pattern.find(text) {
                return (None, &text[m.end()..], false);
            }
        }

        let (first_word, rest, split_on_colon) = split_first_word_at_whitespace_and_colons(text);
        let replacement_word = match case {
            Case::Upper => first_word.to_lowercase(),
            Case::Lower => {
                let mut chars = first_word.chars();
                let first_char = chars.next().unwrap();
                first_char.to_uppercase().collect::<String>() + chars.as_str()
            }
        };

        let start_point = node
            .position
            .as_ref()
            .map(|p| AdjustedOffset::from_unist(&p.start, context))
            .map(|mut p| {
                p.increment(index);
                p
            });
        let end_point = start_point.map(|mut p| {
            p.increment(first_word.len());
            p
        });

        match (start_point, end_point) {
            (Some(start), Some(end)) => {
                let location = AdjustedRange::new(start, end);
                let location = DenormalizedLocation::from_offset_range(location, context);

                (
                    Some(LintFix::Replace(LintFixReplace {
                        location,
                        text: replacement_word,
                    })),
                    rest,
                    split_on_colon,
                )
            }
            _ => (None, rest, split_on_colon),
        }
    }

    fn check_ast(
        &self,
        node: &Node,
        fixes: &mut Vec<LintFix>,
        next_word_capital: &mut NextWordCapital,
        context: &RuleContext,
    ) {
        trace!("Checking ast for node: {node:?} with settings: {next_word_capital:?}");

        fn check_children<T: HasChildren>(
            rule: &Rule001HeadingCase,
            node: &T,
            fixes: &mut Vec<LintFix>,
            next_word_capital: &mut NextWordCapital,
            context: &RuleContext,
        ) {
            node.get_children()
                .iter()
                .for_each(|child| rule.check_ast(child, fixes, next_word_capital, context));
        }

        match node {
            Node::Text(text) => {
                self.check_text_sentence_case(text, fixes, next_word_capital, context)
            }
            Node::Emphasis(emphasis) => {
                check_children(self, emphasis, fixes, next_word_capital, context)
            }
            Node::Link(link) => check_children(self, link, fixes, next_word_capital, context),
            Node::LinkReference(link_reference) => {
                check_children(self, link_reference, fixes, next_word_capital, context)
            }
            Node::Strong(strong) => check_children(self, strong, fixes, next_word_capital, context),
            Node::Heading(heading) => {
                check_children(self, heading, fixes, next_word_capital, context)
            }
            Node::InlineCode(_) => {
                next_word_capital.0 = false;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::parse;

    use super::*;

    #[test]
    fn test_correct_sentence_case() {
        let rule = Rule001HeadingCase::default();
        let mdx = "# This is a correct heading";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_lowercase_first_word() {
        let rule = Rule001HeadingCase::default();
        let mdx = "# this should fail";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
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
                assert_eq!(fix.location.start.row, 0);
                assert_eq!(fix.location.start.column, 2);
                assert_eq!(fix.location.offset_range.start, AdjustedOffset::from(2));
                assert_eq!(fix.location.end.row, 0);
                assert_eq!(fix.location.end.column, 6);
                assert_eq!(fix.location.offset_range.end, AdjustedOffset::from(6));
            }
            _ => panic!("Unexpected fix type"),
        }
    }

    #[test]
    fn test_uppercase_following_words() {
        let rule = Rule001HeadingCase::default();
        let mdx = "# This Should Fail";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
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
                assert_eq!(fix.location.start.row, 0);
                assert_eq!(fix.location.start.column, 7);
                assert_eq!(fix.location.offset_range.start, AdjustedOffset::from(7));
                assert_eq!(fix.location.end.row, 0);
                assert_eq!(fix.location.end.column, 13);
                assert_eq!(fix.location.offset_range.end, AdjustedOffset::from(13));
            }
            _ => panic!("Unexpected fix type"),
        }

        let fix_two = fixes.get(1).unwrap();
        match fix_two {
            LintFix::Replace(fix) => {
                assert_eq!(fix.text, "fail");
                assert_eq!(fix.location.start.row, 0);
                assert_eq!(fix.location.start.column, 14);
                assert_eq!(fix.location.offset_range.start, AdjustedOffset::from(14));
                assert_eq!(fix.location.end.row, 0);
                assert_eq!(fix.location.end.column, 18);
                assert_eq!(fix.location.offset_range.end, AdjustedOffset::from(18));
            }
            _ => panic!("Unexpected fix type"),
        }
    }

    #[test]
    fn test_may_uppercase() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["API"]);
        rule.setup(Some(&settings));

        let mdx = "# This is an API heading";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_may_lowercase() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_lowercase", vec!["the"]);
        rule.setup(Some(&settings));

        let mdx = "# the quick brown fox";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_non_heading_node() {
        let rule = Rule001HeadingCase::default();
        let mdx = "not a heading";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_may_uppercase_multi_word() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["New York City"]);
        rule.setup(Some(&settings));

        let mdx = "# This is about New York City";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_exception_matches() {
        let mut rule = Rule001HeadingCase::default();
        let settings =
            RuleSettings::with_array_of_strings("may_uppercase", vec!["New York", "New York City"]);
        rule.setup(Some(&settings));

        let mdx = "# This is about New York City";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_may_uppercase_partial_match() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["API"]);
        rule.setup(Some(&settings));

        let mdx = "# This is an API-related topic";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_may_lowercase_regex() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_lowercase", vec!["(the|a|an)"]);
        rule.setup(Some(&settings));

        let mdx = "# the quick brown fox";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_may_uppercase_regex_fails() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["[A-Z]{4,}"]);
        rule.setup(Some(&settings));

        let mdx = "# This is an API call";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
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
                assert_eq!(fix.location.start.row, 0);
                assert_eq!(fix.location.start.column, 13);
                assert_eq!(fix.location.offset_range.start, AdjustedOffset::from(13));
                assert_eq!(fix.location.end.row, 0);
                assert_eq!(fix.location.end.column, 16);
                assert_eq!(fix.location.offset_range.end, AdjustedOffset::from(16));
            }
            _ => panic!("Unexpected fix type"),
        }
    }

    #[test]
    fn test_multi_word_exception_at_start() {
        let mut rule = Rule001HeadingCase::default();
        let settings =
            RuleSettings::with_array_of_strings("may_uppercase", vec!["Content Delivery Network"]);
        rule.setup(Some(&settings));

        let mdx = "# Content Delivery Network latency";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_brackets_around_exception() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["Edge Functions"]);
        rule.setup(Some(&settings));

        let mdx = "# Deno (Edge Functions)";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_complex_heading() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["API", "OAuth"]);
        rule.setup(Some(&settings));

        let mdx = "# The basics of API authentication in OAuth";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_can_capitalize_after_colon() {
        let mut rule = Rule001HeadingCase::default();
        rule.setup(None);

        let mdx = "# Bonus: Profile photos";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_can_capitalize_after_colon_with_number() {
        let mut rule = Rule001HeadingCase::default();
        rule.setup(None);

        let mdx = "# Step 1: Do a thing";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_no_flag_inline_code() {
        let mut rule = Rule001HeadingCase::default();
        rule.setup(None);

        let markdown = "# `inline_code` (in a heading) can have `ArbitraryCase`";
        let parse_result = parse(markdown).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_heading_starts_with_number() {
        let mut rule = Rule001HeadingCase::default();
        rule.setup(None);

        let markdown = "# 384 dimensions for vector";
        let parse_result = parse(markdown).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_heading_starts_with_may_uppercase_exception() {
        let mut rule = Rule001HeadingCase::default();
        let settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["API"]);
        rule.setup(Some(&settings));

        let markdown = "### API Error codes";
        let parse_result = parse(markdown).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let result = rule
            .check(
                context.ast().children().unwrap().first().unwrap(),
                &context,
                LintLevel::Error,
            )
            .unwrap();

        let fixes = result.get(0).unwrap().fix.as_ref().unwrap();
        let fix = fixes.get(0).unwrap();
        match fix {
            LintFix::Replace(fix) => {
                assert_eq!(fix.location.start.column, 8);
            }
            _ => panic!("Unexpected fix type"),
        }
    }
}
