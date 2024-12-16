use std::{borrow::Cow, collections::HashSet, ops::Range};

use crop::RopeSlice;
use log::{debug, trace};
use markdown::mdast;
use regex::Regex;
use supa_mdx_macros::RuleName;

use crate::{
    errors::LintError,
    geometry::{AdjustedOffset, AdjustedRange, RangeSet},
    utils::{
        self,
        words::{is_punctuation, BreakOnPunctuation, WordIterator, WordIteratorOptions},
    },
    LintLevel,
};

use super::{
    RegexBeginning, RegexEnding, RegexSettings, Rule, RuleContext, RuleName, RuleSettings,
};

const DICTIONARY: &str = include_str!("./rule003_spelling/dictionary.txt");

enum HyphenatedPart {
    MaybePrefix,
    MaybeSuffix,
}

#[derive(Clone, Default, RuleName)]
pub(crate) struct Rule003Spelling {
    allow_list: Vec<Regex>,
    prefixes: HashSet<String>,
    dictionary: HashSet<String>,
}

impl std::fmt::Debug for Rule003Spelling {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rule003Spelling")
            .field("allow_list", &self.allow_list)
            .field("prefixes", &self.prefixes)
            .field("dictionary", &"[OMITTED (too large)]")
            .finish()
    }
}

impl Rule for Rule003Spelling {
    fn default_level(&self) -> LintLevel {
        LintLevel::Error
    }

    fn setup(&mut self, settings: Option<&RuleSettings>) {
        self.setup_dictionary();

        if let Some(settings) = settings {
            if let Some(vec) = settings.get_array_of_regexes(
                "allow_list",
                Some(&RegexSettings {
                    beginning: Some(RegexBeginning::WordBoundary),
                    ending: Some(RegexEnding::WordBoundary),
                }),
            ) {
                self.allow_list = vec;
            }

            if let Some(vec) = settings.get_array_of_strings("prefixes") {
                self.prefixes = HashSet::from_iter(vec);
            }
        }
    }

    fn check(
        &self,
        ast: &mdast::Node,
        context: &RuleContext,
        level: LintLevel,
    ) -> Option<Vec<LintError>> {
        self.check_node(ast, context, level)
    }
}

impl Rule003Spelling {
    fn message(word: &str) -> String {
        format!("Word not found in dictionary: {}", word)
    }

    fn setup_dictionary(&mut self) {
        let mut set: HashSet<String> = HashSet::new();
        DICTIONARY
            .lines()
            .map(|line| {
                line.split_once(' ')
                    .expect("Every line in static dictionary file should have a space")
                    .0
            })
            .for_each(|word| {
                set.insert(word.to_owned());
            });
        self.dictionary = set;
    }

    fn check_node(
        &self,
        node: &mdast::Node,
        context: &RuleContext,
        level: LintLevel,
    ) -> Option<Vec<LintError>> {
        trace!("[Rule003Spelling] Checking node: {node:#?}");

        let mut errors: Option<Vec<LintError>> = None;

        if let mdast::Node::Text(_) = node {
            if utils::mdast::is_export_const(node) {
                return None;
            };

            if let Some(position) = node.position() {
                let range = AdjustedRange::from_unadjusted_position(position, context);
                let text = context
                    .rope()
                    .byte_slice(Into::<Range<usize>>::into(range.clone()));
                self.check_spelling(text, range.start.into(), context, level, &mut errors);
            }
        }

        errors
    }

    fn check_spelling(
        &self,
        text: RopeSlice,
        text_offset_in_parent: usize,
        context: &RuleContext,
        level: LintLevel,
        errors: &mut Option<Vec<LintError>>,
    ) {
        let text_as_string = text.to_string();
        let mut ignored_ranges: RangeSet = RangeSet::new();
        for exception in self.allow_list.iter() {
            trace!("Checking exception: {exception}");
            let iter = exception.find_iter(&text_as_string);
            for match_result in iter {
                trace!("Found exception match: {match_result:?}");
                ignored_ranges.push(AdjustedRange::new(
                    (match_result.start() + text_offset_in_parent).into(),
                    (match_result.end() + text_offset_in_parent).into(),
                ));
            }
        }
        debug!("Ignored ranges: {ignored_ranges:#?}");

        trace!("Starting tokenizer with text_offset_in_parent: {text_offset_in_parent}");
        let tokenizer =
            WordIterator::new(text, text_offset_in_parent, WordIteratorOptions::default());
        for (offset, word, _cap) in tokenizer {
            let word_as_string = word.to_string();

            let word_range = Self::normalize_word_range(word, offset);
            trace!("Found word {word} in range {word_range:?}");
            if ignored_ranges.completely_contains(&word_range) {
                continue;
            }

            if word_as_string.contains('-') {
                // Deal with hyphenated words
                let mut hyphenated_tokenizer = WordIterator::new(
                    word,
                    offset,
                    WordIteratorOptions {
                        break_on_punctuation: BreakOnPunctuation::Hyphen,
                        ..Default::default()
                    },
                )
                .enumerate()
                .peekable();
                while let Some((idx, (offset, part, _cap))) = hyphenated_tokenizer.next() {
                    if idx == 0 {
                        let adjusted_range =
                            AdjustedRange::new(offset.into(), (offset + part.byte_len()).into());
                        if ignored_ranges.completely_contains(&adjusted_range) {
                            continue;
                        }

                        self.check_word_spelling(
                            &part.to_string(),
                            Some(HyphenatedPart::MaybePrefix),
                            adjusted_range,
                            context,
                            level,
                            errors,
                        );
                    } else if hyphenated_tokenizer.peek().is_none() {
                        let adjusted_range = Self::normalize_word_range(part, offset);
                        if ignored_ranges.completely_contains(&adjusted_range) {
                            continue;
                        }

                        self.check_word_spelling(
                            &part.to_string(),
                            Some(HyphenatedPart::MaybeSuffix),
                            adjusted_range,
                            context,
                            level,
                            errors,
                        );
                    } else {
                        let adjusted_range =
                            AdjustedRange::new(offset.into(), (offset + part.byte_len()).into());
                        if ignored_ranges.completely_contains(&adjusted_range) {
                            continue;
                        }

                        self.check_word_spelling(
                            &part.to_string(),
                            None,
                            adjusted_range,
                            context,
                            level,
                            errors,
                        );
                    }
                }
            } else {
                self.check_word_spelling(&word_as_string, None, word_range, context, level, errors);
            }
        }
    }

    fn check_word_spelling(
        &self,
        word: &str,
        hyphenation: Option<HyphenatedPart>,
        location: AdjustedRange,
        context: &RuleContext,
        level: LintLevel,
        errors: &mut Option<Vec<LintError>>,
    ) {
        if word.len() < 2 {
            return;
        }

        if word
            .chars()
            .any(|c| !c.is_ascii_alphabetic() && !is_punctuation(&c))
        {
            // Ignore words containing non-English alphabet and number
            return;
        }

        let word = Self::normalize_word(word);
        if self.dictionary.contains(word.as_ref()) {
            return;
        }

        if let Some(HyphenatedPart::MaybePrefix) = hyphenation {
            if self.prefixes.contains(word.as_ref()) {
                return;
            }
        }

        let error = LintError::new(
            self.name(),
            Rule003Spelling::message(&word),
            level,
            location,
            None,
            context,
        );
        errors.get_or_insert_with(Vec::new).push(error);
    }

    fn normalize_word_range(word: RopeSlice<'_>, offset: usize) -> AdjustedRange {
        let start: AdjustedOffset = offset.into();
        let mut end: AdjustedOffset = (offset + word.byte_len()).into();

        // 's is too common for us to list every single word that could end with it,
        // just ignore it
        if word.byte_len() > 2
            && word.is_char_boundary(word.byte_len() - 2)
            && word
                .byte_slice(word.byte_len() - 2..)
                .chars()
                .collect::<String>()
                == "'s"
        {
            end -= 2.into();
        }
        // Smart quotes are three bytes long
        else if word.byte_len() > 4 && word.is_char_boundary(word.byte_len() - 4) {
            let ending = word
                .byte_slice(word.byte_len() - 4..)
                .chars()
                .collect::<String>();
            if ending == "‘s" || ending == "’s" {
                end -= 4.into();
            }
        }

        AdjustedRange::new(start, end)
    }

    fn normalize_word(word: &str) -> Cow<str> {
        let mut word = Cow::Borrowed(word);

        let quote_chars = ['‘', '’', '“', '”'];
        if word.contains(|c| quote_chars.contains(&c)) || word.chars().any(|c| c.is_uppercase()) {
            let modified = word
                .replace("‘", "'")
                .replace("’", "'")
                .replace("“", "\"")
                .replace("”", "\"")
                .to_lowercase();
            word = Cow::Owned(modified);
        }

        // 's is too common for us to list every single word that could end with it,
        // just ignore it
        if word.ends_with("'s") {
            match word {
                Cow::Borrowed(s) => Cow::Borrowed(&s[..s.len() - 2]),
                Cow::Owned(mut s) => {
                    s.truncate(s.len() - 2);
                    Cow::Owned(s)
                }
            }
        } else {
            word
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{geometry::AdjustedOffset, parser::parse};

    use super::*;

    #[test]
    fn test_rule003_spelling_good() {
        let mdx = "hello world";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule.check(
            context
                .ast()
                .children()
                .unwrap()
                .get(0)
                .unwrap()
                .children()
                .unwrap()
                .get(0)
                .unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(errors.is_none());
    }

    #[test]
    fn test_rule003_spelling_bad() {
        let mdx = "heloo world";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule
            .check(
                context
                    .ast()
                    .children()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .children()
                    .unwrap()
                    .get(0)
                    .unwrap(),
                &context,
                LintLevel::Error,
            )
            .unwrap();
        assert!(errors.len() == 1);

        let error = &errors[0];
        assert_eq!(error.message, "Word not found in dictionary: heloo");
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(0));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(5));
    }

    #[test]
    fn test_rule003_with_exception() {
        let mdx = "heloo world";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        let settings = RuleSettings::with_array_of_strings("allow_list", vec!["heloo"]);
        rule.setup(Some(&settings));

        let errors = rule.check(
            context
                .ast()
                .children()
                .unwrap()
                .get(0)
                .unwrap()
                .children()
                .unwrap()
                .get(0)
                .unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(errors.is_none());
    }

    #[test]
    fn test_rule003_with_repeated_exception() {
        let mdx = "heloo world heloo";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        let settings = RuleSettings::with_array_of_strings("allow_list", vec!["heloo"]);
        rule.setup(Some(&settings));

        let errors = rule.check(
            context
                .ast()
                .children()
                .unwrap()
                .get(0)
                .unwrap()
                .children()
                .unwrap()
                .get(0)
                .unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(errors.is_none());
    }

    #[test]
    fn test_rule003_with_regex_exception() {
        let mdx = "Heloo world";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        let settings = RuleSettings::with_array_of_strings("allow_list", vec!["[Hh]eloo"]);
        rule.setup(Some(&settings));

        let errors = rule.check(
            context
                .ast()
                .children()
                .unwrap()
                .get(0)
                .unwrap()
                .children()
                .unwrap()
                .get(0)
                .unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(errors.is_none());
    }

    #[test]
    fn test_rule003_with_punctuation() {
        let mdx = "heloo, 'asdf' world";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule
            .check(
                context
                    .ast()
                    .children()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .children()
                    .unwrap()
                    .get(0)
                    .unwrap(),
                &context,
                LintLevel::Error,
            )
            .unwrap();
        assert!(errors.len() == 2);

        let error = &errors[0];
        assert_eq!(error.message, "Word not found in dictionary: heloo");
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(0));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(5));

        let error = &errors[1];
        assert_eq!(error.message, "Word not found in dictionary: asdf");
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(8));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(12));
    }

    #[test]
    fn test_rule003_with_midword_punctuation() {
        // Shouldn't is in dictionary, but hell'o is not
        let mdx = "hell'o world shouldn't work";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule
            .check(
                context
                    .ast()
                    .children()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .children()
                    .unwrap()
                    .get(0)
                    .unwrap(),
                &context,
                LintLevel::Error,
            )
            .unwrap();
        assert!(errors.len() == 1);

        let error = &errors[0];
        assert_eq!(error.message, "Word not found in dictionary: hell'o");
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(0));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(6));
    }

    #[test]
    fn test_rule003_with_multiple_lines() {
        let mdx = "hello world\nhello world\nheloo world";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule
            .check(
                context
                    .ast()
                    .children()
                    .unwrap()
                    .get(0)
                    .unwrap()
                    .children()
                    .unwrap()
                    .get(0)
                    .unwrap(),
                &context,
                LintLevel::Error,
            )
            .unwrap();
        assert!(errors.len() == 1);

        let error = &errors[0];
        assert_eq!(error.message, "Word not found in dictionary: heloo");
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(24));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(29));
    }

    #[test]
    fn test_rule003_with_prefix() {
        let mdx = "hello pre-world";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        let settings = RuleSettings::with_array_of_strings("prefixes", vec!["pre"]);
        rule.setup(Some(&settings));

        let errors = rule.check(
            context
                .ast()
                .children()
                .unwrap()
                .get(0)
                .unwrap()
                .children()
                .unwrap()
                .get(0)
                .unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(errors.is_none());
    }

    #[test]
    fn test_rule003_ignore_filenames() {
        let mdx = "use the file hello.toml";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        let settings = RuleSettings::with_array_of_strings("allow_list", vec!["\\S+\\.toml"]);
        rule.setup(Some(&settings));

        let errors = rule.check(
            context
                .ast()
                .children()
                .unwrap()
                .get(0)
                .unwrap()
                .children()
                .unwrap()
                .get(0)
                .unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(errors.is_none());
    }

    #[test]
    fn test_rule003_ignore_complex_regex() {
        let mdx = "test a thing [#rest-api-overview]";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        let settings = RuleSettings::with_array_of_strings("allow_list", vec!["\\[#[A-Za-z-]+\\]"]);
        rule.setup(Some(&settings));

        let errors = rule.check(
            context
                .ast()
                .children()
                .unwrap()
                .get(0)
                .unwrap()
                .children()
                .unwrap()
                .get(0)
                .unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(errors.is_none());
    }

    #[test]
    fn test_rule003_ignore_emojis() {
        let mdx = "hello 🤝 world";
        let parse_result = parse(mdx).unwrap();
        let context = RuleContext::new(parse_result, None).unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule.check(
            context
                .ast()
                .children()
                .unwrap()
                .get(0)
                .unwrap()
                .children()
                .unwrap()
                .get(0)
                .unwrap(),
            &context,
            LintLevel::Error,
        );
        assert!(errors.is_none());
    }
}
