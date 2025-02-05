use std::{cell::RefCell, ops::Range};

use crop::RopeSlice;
use log::debug;
use markdown::mdast::{Node, Text};
use regex::Regex;
use supa_mdx_macros::RuleName;

use crate::{
    errors::{LintError, LintLevel},
    fix::{LintCorrection, LintCorrectionReplace},
    geometry::{AdjustedOffset, AdjustedRange, DenormalizedLocation},
    utils::{
        mdast::HasChildren,
        words::{Capitalize, CapitalizeTriggerPunctuation, WordIterator, WordIteratorOptions},
    },
};

use super::{
    RegexBeginning, RegexEnding, RegexSettings, Rule, RuleContext, RuleName, RuleSettings,
};

/// Headings should be in sentence case.
///
/// ## Examples
///
/// ### Valid
///
/// ```markdown
/// # This is sentence case
/// ```
///
/// ### Invalid
///
/// ```markdown
/// # This is Not Sentence Case
/// ```
///
/// ## Exceptions
///
/// Exceptions are configured via the `may_uppercase` and `may_lowercase` arrays.
/// - `may_uppercase`: Words that may be capitalized even if they are not the first word in the heading.
/// - `may_lowercase`: Words that may be lowercased even if they are the first word in the heading.
///
/// See an  [example from the Supabase repo](https://github.com/supabase/supabase/blob/master/supa-mdx-lint/Rule001HeadingCase.toml).
#[derive(Debug, RuleName)]
pub struct Rule001HeadingCase {
    may_uppercase: Vec<Regex>,
    may_lowercase: Vec<Regex>,
    next_word_capital: RefCell<Capitalize>,
}

impl Default for Rule001HeadingCase {
    fn default() -> Self {
        Self {
            may_uppercase: Vec::new(),
            may_lowercase: Vec::new(),
            next_word_capital: RefCell::new(Capitalize::True),
        }
    }
}

impl Rule for Rule001HeadingCase {
    fn default_level(&self) -> LintLevel {
        LintLevel::Error
    }

    fn setup(&mut self, settings: Option<&mut RuleSettings>) {
        if let Some(settings) = settings {
            let regex_settings = RegexSettings {
                beginning: Some(RegexBeginning::VeryBeginning),
                ending: Some(RegexEnding::WordBoundary),
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

        self.reset_mutable_state();

        let mut fixes: Option<Vec<LintCorrection>> = None;
        self.check_ast(ast, &mut fixes, context);
        fixes
            .and_then(|fixes| {
                LintError::from_node()
                    .node(ast)
                    .context(context)
                    .rule(self.name())
                    .level(level)
                    .message(&self.message())
                    .fix(fixes)
                    .call()
            })
            .map(|error| vec![error])
    }
}

impl Rule001HeadingCase {
    fn message(&self) -> String {
        "Heading should be sentence case".to_string()
    }

    fn reset_mutable_state(&self) {
        self.next_word_capital.replace(Capitalize::True);
    }

    fn check_text_sentence_case(
        &self,
        text: &Text,
        fixes: &mut Option<Vec<LintCorrection>>,
        context: &RuleContext,
    ) {
        if let Some(position) = text.position.as_ref() {
            let range = AdjustedRange::from_unadjusted_position(position, context);
            let rope = context.rope().byte_slice(Into::<Range<usize>>::into(range));

            let mut word_iterator = WordIterator::new(
                rope,
                0,
                WordIteratorOptions {
                    initial_capitalize: *self.next_word_capital.borrow(),
                    capitalize_trigger_punctuation: CapitalizeTriggerPunctuation::PlusColon,
                    ..Default::default()
                },
            );

            let mut first_word = *self.next_word_capital.borrow() == Capitalize::True;

            while let Some((offset, word, cap)) = word_iterator.next() {
                debug!("Got next word: {word:?} at offset {offset} with capitalization {cap:?}");
                if word.is_empty() {
                    continue;
                }

                match cap {
                    Capitalize::True => {
                        if word.chars().next().unwrap().is_lowercase()
                            && !self.handle_exception_match(
                                rope.byte_slice(offset..),
                                offset,
                                cap,
                                &mut word_iterator,
                            )
                        {
                            self.create_text_lint_fix(
                                word.to_string(),
                                text,
                                offset,
                                cap,
                                context,
                                fixes,
                            );
                        } else if first_word {
                            self.handle_exception_match(
                                rope.byte_slice(offset..),
                                offset,
                                Capitalize::False,
                                &mut word_iterator,
                            );
                        }
                    }
                    Capitalize::False => {
                        if word.chars().next().unwrap().is_uppercase()
                            && !self.handle_exception_match(
                                rope.byte_slice(offset..),
                                offset,
                                cap,
                                &mut word_iterator,
                            )
                        {
                            self.create_text_lint_fix(
                                word.to_string(),
                                text,
                                offset,
                                cap,
                                context,
                                fixes,
                            );
                        }
                    }
                }

                first_word = false;
                self.next_word_capital
                    .replace(word_iterator.next_capitalize().unwrap());
            }
        }
    }

    fn handle_exception_match(
        &self,
        rope: RopeSlice<'_>,
        offset: usize,
        capitalize: Capitalize,
        word_iterator: &mut WordIterator<'_>,
    ) -> bool {
        let patterns = match capitalize {
            Capitalize::True => &self.may_lowercase,
            Capitalize::False => &self.may_uppercase,
        };

        let text = rope.to_string();
        debug!("Checking for exceptions in {text}");
        for pattern in patterns {
            if let Some(match_result) = pattern.find(&text) {
                debug!("Found exception match: {match_result:?}");
                while offset + match_result.len()
                    > word_iterator
                        .curr_index()
                        .expect("WordIterator index should not be queried while unstable")
                {
                    if word_iterator.next().is_none() {
                        break;
                    }
                }

                return true;
            }
        }

        false
    }

    fn create_text_lint_fix(
        &self,
        word: String,
        node: &Text,
        offset: usize,
        capitalize: Capitalize,
        context: &RuleContext,
        fixes: &mut Option<Vec<LintCorrection>>,
    ) {
        let replacement_word = match capitalize {
            Capitalize::True => {
                let mut chars = word.chars();
                let first_char = chars.next().unwrap();
                first_char.to_uppercase().collect::<String>() + chars.as_str()
            }
            Capitalize::False => word.to_lowercase(),
        };

        let start_point = node
            .position
            .as_ref()
            .map(|p| AdjustedOffset::from_unist(&p.start, context))
            .map(|mut p| {
                p.increment(offset);
                p
            });
        let end_point = start_point.map(|mut p| {
            p.increment(word.len());
            p
        });

        if let (Some(start), Some(end)) = (start_point, end_point) {
            let location = AdjustedRange::new(start, end);
            let location = DenormalizedLocation::from_offset_range(location, context);

            let fix = LintCorrection::Replace(LintCorrectionReplace {
                location,
                text: replacement_word,
            });
            fixes.get_or_insert_with(Vec::new).push(fix);
        }
    }

    fn check_ast(
        &self,
        node: &Node,
        fixes: &mut Option<Vec<LintCorrection>>,
        context: &RuleContext,
    ) {
        debug!(
            "Checking ast for node: {node:?} with next word capital: {:?}",
            self.next_word_capital
        );

        fn check_children<T: HasChildren>(
            rule: &Rule001HeadingCase,
            node: &T,
            fixes: &mut Option<Vec<LintCorrection>>,
            context: &RuleContext,
        ) {
            node.get_children()
                .iter()
                .for_each(|child| rule.check_ast(child, fixes, context));
        }

        match node {
            Node::Text(text) => self.check_text_sentence_case(text, fixes, context),
            Node::Emphasis(emphasis) => check_children(self, emphasis, fixes, context),
            Node::Link(link) => check_children(self, link, fixes, context),
            Node::LinkReference(link_reference) => {
                check_children(self, link_reference, fixes, context)
            }
            Node::Strong(strong) => check_children(self, strong, fixes, context),
            Node::Heading(heading) => check_children(self, heading, fixes, context),
            Node::InlineCode(_) => {
                self.next_word_capital.replace(Capitalize::False);
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
    fn test_rule001_correct_sentence_case() {
        let rule = Rule001HeadingCase::default();
        let mdx = "# This is a correct heading";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_lowercase_first_word() {
        let rule = Rule001HeadingCase::default();
        let mdx = "# this should fail";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

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
            LintCorrection::Replace(fix) => {
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
    fn test_rule001_uppercase_following_words() {
        let rule = Rule001HeadingCase::default();
        let mdx = "# This Should Fail";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

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
            LintCorrection::Replace(fix) => {
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
            LintCorrection::Replace(fix) => {
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
    fn test_rule001_may_uppercase() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["API"]);
        rule.setup(Some(&mut settings));

        let mdx = "# This is an API heading";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_may_lowercase() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = RuleSettings::with_array_of_strings("may_lowercase", vec!["the"]);
        rule.setup(Some(&mut settings));

        let mdx = "# the quick brown fox";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_non_heading_node() {
        let rule = Rule001HeadingCase::default();
        let mdx = "not a heading";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_may_uppercase_multi_word() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings =
            RuleSettings::with_array_of_strings("may_uppercase", vec!["New York City"]);
        rule.setup(Some(&mut settings));

        let mdx = "# This is about New York City";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_multiple_exception_matches() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings =
            RuleSettings::with_array_of_strings("may_uppercase", vec!["New York", "New York City"]);
        rule.setup(Some(&mut settings));

        let mdx = "# This is about New York City";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_may_uppercase_partial_match() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["API"]);
        rule.setup(Some(&mut settings));

        let mdx = "# This is an API-related topic";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_may_lowercase_regex() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = RuleSettings::with_array_of_strings("may_lowercase", vec!["(the|a|an)"]);
        rule.setup(Some(&mut settings));

        let mdx = "# the quick brown fox";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_may_uppercase_regex_fails() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["[A-Z]{4,}"]);
        rule.setup(Some(&mut settings));

        let mdx = "# This is an API call";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

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
            LintCorrection::Replace(fix) => {
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
    fn test_rule001_multi_word_exception_at_start() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings =
            RuleSettings::with_array_of_strings("may_uppercase", vec!["Content Delivery Network"]);
        rule.setup(Some(&mut settings));

        let mdx = "# Content Delivery Network latency";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_multi_word_exception_in_middle() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["Magic Link"]);
        rule.setup(Some(&mut settings));

        let markdown = "### Enabling Magic Link signins";
        let parse_result = parse(markdown).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );

        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_brackets_around_exception() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings =
            RuleSettings::with_array_of_strings("may_uppercase", vec!["Edge Functions"]);
        rule.setup(Some(&mut settings));

        let mdx = "# Deno (Edge Functions)";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_complex_heading() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings =
            RuleSettings::with_array_of_strings("may_uppercase", vec!["API", "OAuth"]);
        rule.setup(Some(&mut settings));

        let mdx = "# The basics of API authentication in OAuth";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_can_capitalize_after_colon() {
        let mut rule = Rule001HeadingCase::default();
        rule.setup(None);

        let mdx = "# Bonus: Profile photos";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_can_capitalize_after_colon_with_number() {
        let mut rule = Rule001HeadingCase::default();
        rule.setup(None);

        let mdx = "# Step 1: Do a thing";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_can_capitalize_after_sentence_break() {
        let mut rule = Rule001HeadingCase::default();
        rule.setup(None);

        let mdx = "# 1. Do a thing";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_no_flag_inline_code() {
        let mut rule = Rule001HeadingCase::default();
        rule.setup(None);

        let markdown = "# `inline_code` (in a heading) can have `ArbitraryCase`";
        let parse_result = parse(markdown).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_heading_starts_with_number() {
        let mut rule = Rule001HeadingCase::default();
        rule.setup(None);

        let markdown = "# 384 dimensions for vector";
        let parse_result = parse(markdown).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_rule001_heading_starts_with_may_uppercase_exception() {
        let mut rule = Rule001HeadingCase::default();
        let mut settings = RuleSettings::with_array_of_strings("may_uppercase", vec!["API"]);
        rule.setup(Some(&mut settings));

        let markdown = "### API Error codes";
        let parse_result = parse(markdown).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

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
            LintCorrection::Replace(fix) => {
                assert_eq!(fix.location.start.column, 8);
            }
            _ => panic!("Unexpected fix type"),
        }
    }

    #[test]
    fn test_rule001_heading_contains_link() {
        let mut rule = Rule001HeadingCase::default();
        rule.setup(None);

        let markdown = "## Filtering with [regular expressions](https://en.wikipedia.org/wiki/Regular_expression)";
        let parse_result = parse(markdown).unwrap();
        let context = RuleContext::builder()
            .parse_result(parse_result)
            .build()
            .unwrap();

        let result = rule.check(
            context.ast().children().unwrap().first().unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(result.is_none());
    }
}
