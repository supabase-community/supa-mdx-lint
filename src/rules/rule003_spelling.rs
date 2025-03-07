use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, HashSet},
    ops::Range,
    rc::Rc,
};

use crop::RopeSlice;
use log::{debug, trace};
use markdown::mdast;
use regex::Regex;
use suggestions::SuggestionMatcher;
use supa_mdx_macros::RuleName;

use crate::{
    comments::LintTimeRuleConfigs,
    context::{Context, ContextId},
    errors::LintError,
    fix::{LintCorrection, LintCorrectionReplace},
    geometry::{
        AdjustedOffset, AdjustedRange, DenormalizedLocation, MaybeEndedLineRange, RangeSet,
    },
    utils::{
        self,
        lru::LruCache,
        regex::expand_regex,
        words::{is_punctuation, BreakOnPunctuation, WordIterator, WordIteratorOptions},
    },
    LintLevel,
};

use super::{RegexBeginning, RegexEnding, RegexSettings, Rule, RuleName, RuleSettings};

mod suggestions;

const DICTIONARY: &str = include_str!("./rule003_spelling/dictionary.txt");

#[derive(Debug, Clone)]
enum HyphenatedPart {
    MaybePrefix,
    MaybeSuffix,
}

#[derive(Debug, Default)]
struct LintTimeVocabAllowed(HashMap<String, Vec<MaybeEndedLineRange>>);

/// Words are checked for correct spelling.
///
/// There are four ways to deal with words that are flagged, even though you're spelling them as intended:
///
/// 1. For proper nouns and jargon, you can add them to the [Vocabulary](#vocabulary).
/// 2. For function, method, and variable names, you can format them as inline code. For example, instead of `foo`, write `` `foo` ``.
/// 3. You can add a temporary configuration, which will take effect for either the next line or the rest of the file. This configuration adds the specified words to the vocabulary temporarily. Words added are case-sensitive.
///    ```markdown
///    {/* supa-mdx-lint-configure Rule003Spelling +Supabase */}
///    {/* supa-mdx-lint-configure-next-line Rule003Spelling +pgTAP */}
///    ```
/// 4. You can disable the rule by using one of the disable directives. This should be used as a last resort.
///    ```markdown
///    {/* supa-mdx-lint-disable Rule003Spelling */}
///    {/* supa-mdx-lint-disable-next-line Rule003Spelling */}
///    ```
///
/// ## Examples
///
/// ### Valid
///
/// ```markdown
/// This is correctly spelled.
/// ```
///
/// ### Invalid
///
/// ```markdown
/// This is incorrectyl spelled.
/// ```
///
/// ## Vocabulary
///
/// Vocabulary can be added via the `allow_list` and `prefixes` arrays.
///
/// - `allow_list`: A list of words (or regex patterns to match words) that are considered correctly spelled.
/// - `prefixes`: A list of prefixes that are not standalone words, but that can be used in a prefix before a hyphen (e.g., `pre`, `bi`).
///
/// See an  [example from the Supabase repo](https://github.com/supabase/supabase/blob/master/supa-mdx-lint/Rule003Spelling.toml).
#[derive(Default, RuleName)]
pub struct Rule003Spelling {
    allow_list: Vec<Regex>,
    prefixes: HashSet<String>,
    dictionary: HashSet<String>,
    config_cache: Rc<RefCell<LruCache<ContextId, Option<LintTimeVocabAllowed>>>>,
    suggestion_matcher: SuggestionMatcher,
}

impl std::fmt::Debug for Rule003Spelling {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rule003Spelling")
            .field("allow_list", &self.allow_list)
            .field("prefixes", &self.prefixes)
            .field("configuration_cache", &self.config_cache)
            .field("dictionary", &"[OMITTED (too large)]")
            .finish()
    }
}

impl Rule for Rule003Spelling {
    fn default_level(&self) -> LintLevel {
        LintLevel::Error
    }

    fn setup(&mut self, settings: Option<&mut RuleSettings>) {
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

        self.setup_dictionary();
    }

    fn check(
        &self,
        ast: &mdast::Node,
        context: &Context,
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

        let custom_words = self
            .allow_list
            .iter()
            .flat_map(|regex| {
                expand_regex()
                    .regex(regex.as_str())
                    .call()
                    .into_iter()
                    .flatten()
            })
            .collect::<Vec<_>>();
        let suggestion_matcher = SuggestionMatcher::new(&custom_words);
        self.suggestion_matcher = suggestion_matcher;
    }

    /// Parse lint-time configuration comments for this rule.
    ///
    /// ## Examples
    ///
    /// 1. Allows "Supabase" for the rest of the file:
    ///    ```mdx
    ///    {/* supa-mdx-lint-configure Rule003Spelling +Supabase */}
    ///    ```
    /// 1. Allows "Supabase" for the next line:
    ///    ```mdx
    ///    {/* supa-mdx-lint-configure-next-line Rule003Spelling +Supabase */}
    ///    ```
    fn parse_lint_time_config(&self, cache_key: &ContextId, config: &LintTimeRuleConfigs) {
        if self.config_cache.borrow().contains_key(cache_key) {
            return;
        }

        let map = config.get(&self.name().into()).map(|list| {
            let mut map = HashMap::new();
            for (word, range) in list {
                if !word.starts_with('+') {
                    continue;
                }
                let word = word.trim_start_matches('+');
                map.entry(word.to_string())
                    .or_insert_with(Vec::new)
                    .push(range.clone());
            }
            LintTimeVocabAllowed(map)
        });
        self.config_cache
            .borrow_mut()
            .insert(cache_key.clone(), map);
    }

    fn with_lint_time_config<F, R>(&self, cache_key: &ContextId, f: F) -> Option<R>
    where
        F: FnOnce(&LintTimeVocabAllowed) -> R,
    {
        self.config_cache
            .borrow_mut()
            .get(cache_key)?
            .as_ref()
            .map(f)
    }

    fn check_node(
        &self,
        node: &mdast::Node,
        context: &Context,
        level: LintLevel,
    ) -> Option<Vec<LintError>> {
        trace!("[Rule003Spelling] Checking node: {node:#?}");

        let mut errors: Option<Vec<LintError>> = None;

        if let mdast::Node::Text(_) = node {
            if utils::mdast::is_export_const(node) {
                return None;
            };

            if let Some(position) = node.position() {
                self.parse_lint_time_config(&context.key, &context.lint_time_rule_configs);

                let range = AdjustedRange::from_unadjusted_position(position, context);
                let text = context
                    .rope()
                    .byte_slice(Into::<Range<usize>>::into(range.clone()));
                self.check_spelling(text, range.start.into(), context, level, &mut errors);
            }
        }

        errors
    }

    fn get_ignored_ranges(&self, text: &str, offset: usize, ctx: &Context) -> RangeSet {
        let mut ignored_ranges: RangeSet = RangeSet::new();
        for exception in self.allow_list.iter() {
            trace!("Checking exception: {exception}");
            let iter = exception.find_iter(text);
            for match_result in iter {
                trace!("Found exception match: {match_result:?}");
                ignored_ranges.push(AdjustedRange::new(
                    (match_result.start() + offset).into(),
                    (match_result.end() + offset).into(),
                ));
            }
        }
        self.with_lint_time_config(&ctx.key, |config| {
            config.0.iter().for_each(|(word, ranges)| {
                let word_pattern =
                    regex::Regex::new(&format!(r"\b{}\b", regex::escape(word))).unwrap();
                for r#match in word_pattern.find_iter(text) {
                    let word_start = r#match.start() + offset;
                    let word_end = r#match.end() + offset;
                    let word_range = AdjustedRange::new(word_start.into(), word_end.into());

                    for range in ranges {
                        if range.overlaps_lines(&word_range, ctx.rope()) {
                            ignored_ranges.push(word_range.clone());
                        }
                    }
                }
            })
        });
        ignored_ranges
    }

    fn check_spelling(
        &self,
        text: RopeSlice,
        text_offset_in_parent: usize,
        context: &Context,
        level: LintLevel,
        errors: &mut Option<Vec<LintError>>,
    ) {
        let text_as_string = text.to_string();
        let ignored_ranges =
            self.get_ignored_ranges(&text_as_string, text_offset_in_parent, context);
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

            if word_as_string.contains('-') && !self.is_correct_spelling(&word_as_string, &None) {
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
        context: &Context,
        level: LintLevel,
        errors: &mut Option<Vec<LintError>>,
    ) {
        if self.is_correct_spelling(word, &hyphenation) {
            return;
        }

        let suggestions = match hyphenation {
            None => {
                let suggestions = self.suggestion_matcher.suggest(word);
                if suggestions.is_empty() {
                    None
                } else {
                    Some(
                        suggestions
                            .into_iter()
                            .map(|s| {
                                LintCorrection::Replace(LintCorrectionReplace {
                                    text: s,
                                    location: DenormalizedLocation::from_offset_range(
                                        location.clone(),
                                        context,
                                    ),
                                })
                            })
                            .collect::<Vec<_>>(),
                    )
                }
            }
            Some(_) => None,
        };

        let error = LintError::builder()
            .rule(self.name())
            .message(Rule003Spelling::message(word))
            .level(level)
            .location(location)
            .context(context)
            .maybe_suggestions(suggestions)
            .build();
        errors.get_or_insert_with(Vec::new).push(error);
    }

    fn is_correct_spelling(&self, word: &str, hyphenation: &Option<HyphenatedPart>) -> bool {
        trace!("Checking spelling of word: {word} with hyphenation: {hyphenation:?}");
        if word.len() < 2 {
            return true;
        }

        if word
            .chars()
            .any(|c| !c.is_ascii_alphabetic() && !Self::is_included_punctuation(&c))
        {
            return true;
        }

        let word = Self::normalize_word(word);
        if self.dictionary.contains(word.as_ref()) {
            return true;
        }

        if let Some(HyphenatedPart::MaybePrefix) = hyphenation {
            if self.prefixes.contains(word.as_ref()) {
                return true;
            }
        }

        false
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

    fn is_included_punctuation(c: &char) -> bool {
        is_punctuation(c)
            && (*c == '-'
                || *c == '–'
                || *c == '—'
                || *c == '―'
                || *c == '\''
                || *c == '‘'
                || *c == '’'
                || *c == '“'
                || *c == '”'
                || *c == '"'
                || *c == '.')
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule.check(
            parse_result
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule
            .check(
                parse_result
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        let mut settings = RuleSettings::with_array_of_strings("allow_list", vec!["heloo"]);
        rule.setup(Some(&mut settings));

        let errors = rule.check(
            parse_result
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        let mut settings = RuleSettings::with_array_of_strings("allow_list", vec!["heloo"]);
        rule.setup(Some(&mut settings));

        let errors = rule.check(
            parse_result
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        let mut settings = RuleSettings::with_array_of_strings("allow_list", vec!["[Hh]eloo"]);
        rule.setup(Some(&mut settings));

        let errors = rule.check(
            parse_result
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule
            .check(
                parse_result
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule
            .check(
                parse_result
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule
            .check(
                parse_result
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        let mut settings = RuleSettings::with_array_of_strings("prefixes", vec!["pre"]);
        rule.setup(Some(&mut settings));

        let errors = rule.check(
            parse_result
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        let mut settings = RuleSettings::with_array_of_strings("allow_list", vec!["\\S+\\.toml"]);
        rule.setup(Some(&mut settings));

        let errors = rule.check(
            parse_result
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        let mut settings =
            RuleSettings::with_array_of_strings("allow_list", vec!["\\[#[A-Za-z-]+\\]"]);
        rule.setup(Some(&mut settings));

        let errors = rule.check(
            parse_result
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
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule.check(
            parse_result
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
    fn test_rule003_bare_prefixes() {
        let mdx = "pre- and post-world";
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        let mut settings = RuleSettings::with_array_of_strings("prefixes", vec!["pre", "post"]);
        rule.setup(Some(&mut settings));

        let errors = rule.check(
            parse_result
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
    fn test_rule003_suggestions() {
        let mdx = "heloo wrld";
        let parse_result = parse(mdx).unwrap();
        let context = Context::builder()
            .parse_result(&parse_result)
            .build()
            .unwrap();

        let mut rule = Rule003Spelling::default();
        rule.setup(None);

        let errors = rule
            .check(
                parse_result
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
        assert!(error.suggestions.is_some());
        let suggestions = error.suggestions.as_ref().unwrap();
        assert!(suggestions.iter().any(|s| match s {
            LintCorrection::Replace(replace) => replace.text == "hello",
            _ => false,
        }));

        let error = &errors[1];
        assert_eq!(error.message, "Word not found in dictionary: wrld");
        assert_eq!(error.location.offset_range.start, AdjustedOffset::from(6));
        assert_eq!(error.location.offset_range.end, AdjustedOffset::from(10));
        assert!(error.suggestions.is_some());
        let suggestions = error.suggestions.as_ref().unwrap();
        assert!(suggestions.iter().any(|s| match s {
            LintCorrection::Replace(replace) => replace.text == "world",
            _ => false,
        }));
    }
}
